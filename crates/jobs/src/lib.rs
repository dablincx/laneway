//! laneway: background job queue on Postgres. No Redis, no extra service.
//!
//! - Jobs are rows in `jobs`. Enqueue them in the same transaction as the change that caused
//!   them (transactional outbox): if the transaction rolls back, the job never existed.
//! - Workers claim jobs with `FOR UPDATE SKIP LOCKED` and wake up on `NOTIFY laneway_jobs`.
//! - **Serial keys:** jobs with the same serial key run one at a time, in enqueue order.
//! - **Replace keys:** enqueueing replaces a still-queued job of the same kind + key, which
//!   gives debouncing (a timer that resets on every enqueue).
//! - **Retries:** failed jobs come back with exponential backoff until `max_attempts`.
//! - **Leases:** a running job holds a lease that the worker renews. If the worker dies, the
//!   lease runs out and the job is queued again.

mod worker;

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

pub use worker::{Handler, JobContext, JobError, Registry, Worker};

/// Postgres channel workers listen on.
pub const NOTIFY_CHANNEL: &str = "laneway_jobs";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't serialize job payload: {0}")]
    Payload(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// A job to enqueue.
#[derive(Debug, Clone)]
pub struct NewJob {
    kind: String,
    payload: serde_json::Value,
    serial_key: Option<String>,
    replace_key: Option<String>,
    run_at: Option<DateTime<Utc>>,
    max_attempts: i32,
}

impl NewJob {
    pub fn new(kind: impl Into<String>, payload: &impl Serialize) -> Result<Self, Error> {
        Ok(Self {
            kind: kind.into(),
            payload: serde_json::to_value(payload)?,
            serial_key: None,
            replace_key: None,
            run_at: None,
            max_attempts: 5,
        })
    }

    /// Run one at a time with other jobs that have the same key, in enqueue order.
    #[must_use]
    pub fn serial(mut self, key: impl Into<String>) -> Self {
        self.serial_key = Some(key.into());
        self
    }

    /// Replace a still-queued job of the same kind with this key instead of adding a new one.
    #[must_use]
    pub fn replace(mut self, key: impl Into<String>) -> Self {
        self.replace_key = Some(key.into());
        self
    }

    /// Don't run before this time.
    #[must_use]
    pub fn run_at(mut self, at: DateTime<Utc>) -> Self {
        self.run_at = Some(at);
        self
    }

    /// Don't run before `now + delay`.
    #[must_use]
    pub fn delay(self, delay: Duration) -> Self {
        let at = Utc::now() + chrono::Duration::from_std(delay).unwrap_or(chrono::Duration::MAX);
        self.run_at(at)
    }

    #[must_use]
    pub fn max_attempts(mut self, attempts: i32) -> Self {
        self.max_attempts = attempts.max(1);
        self
    }
}

/// Enqueues a job. Pass a transaction to enqueue atomically with other changes; workers are
/// notified when it commits. Returns the job id (the existing id when a job was replaced).
pub async fn enqueue<'e>(executor: impl PgExecutor<'e>, job: NewJob) -> Result<Uuid, Error> {
    let id = sqlx::query_scalar!(
        r#"
        WITH inserted AS (
            INSERT INTO jobs (id, kind, payload, serial_key, replace_key, run_at, max_attempts)
            VALUES ($1, $2, $3, $4, $5, COALESCE($6, now()), $7)
            ON CONFLICT (kind, replace_key) WHERE status = 'queued' AND replace_key IS NOT NULL
            DO UPDATE SET payload = EXCLUDED.payload, run_at = EXCLUDED.run_at,
                          serial_key = EXCLUDED.serial_key, max_attempts = EXCLUDED.max_attempts
            RETURNING id, kind
        )
        SELECT id AS "id!" FROM inserted, LATERAL (SELECT pg_notify($8, inserted.kind)) AS n
        "#,
        laneway_db::new_id(),
        job.kind,
        job.payload,
        job.serial_key,
        job.replace_key,
        job.run_at,
        job.max_attempts,
        NOTIFY_CHANNEL,
    )
    .fetch_one(executor)
    .await?;
    Ok(id)
}

/// A claimed job.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    /// 1 on the first run.
    pub attempt: i32,
    pub max_attempts: i32,
}

/// Claims the next due job of one of `kinds`. `Ok(None)` = nothing to do right now.
pub async fn claim(
    pool: &PgPool,
    worker_id: &str,
    kinds: &[String],
    lease: Duration,
) -> Result<Option<Job>, Error> {
    let lease_seconds = lease.as_secs_f64();
    let result = sqlx::query_as!(
        Job,
        r#"
        UPDATE jobs
        SET status = 'running', attempts = attempts + 1, locked_by = $1,
            lease_until = now() + make_interval(secs => $2)
        WHERE id = (
            SELECT j.id FROM jobs j
            WHERE j.status = 'queued' AND j.run_at <= now() AND j.kind = ANY($3)
              AND (j.serial_key IS NULL OR (
                    NOT EXISTS (SELECT 1 FROM jobs r
                                WHERE r.serial_key = j.serial_key AND r.status = 'running')
                AND NOT EXISTS (SELECT 1 FROM jobs e
                                WHERE e.serial_key = j.serial_key AND e.status = 'queued' AND e.id < j.id)))
            ORDER BY j.run_at, j.id
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        RETURNING id, kind, payload, attempts AS attempt, max_attempts
        "#,
        worker_id,
        lease_seconds,
        kinds,
    )
    .fetch_optional(pool)
    .await;

    match result {
        Ok(job) => Ok(job),
        // Another worker started a job with the same serial key at the same moment.
        Err(sqlx::Error::Database(e)) if e.constraint() == Some("jobs_serial_running_uidx") => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Extends the lease of a running job. Returns false if the job isn't ours anymore.
pub async fn renew(pool: &PgPool, id: Uuid, worker_id: &str, lease: Duration) -> Result<bool, Error> {
    let done = sqlx::query!(
        "UPDATE jobs SET lease_until = now() + make_interval(secs => $3)
         WHERE id = $1 AND status = 'running' AND locked_by = $2",
        id,
        worker_id,
        lease.as_secs_f64(),
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Marks a job as done.
pub async fn complete(pool: &PgPool, id: Uuid) -> Result<(), Error> {
    sqlx::query!(
        "UPDATE jobs SET status = 'done', finished_at = now(), lease_until = NULL, last_error = NULL
         WHERE id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Delay before retry number `attempt` (1-based): 10s, 20s, 40s, ... capped at 1 hour.
pub fn backoff(attempt: i32) -> Duration {
    let exp = u32::try_from(attempt.clamp(1, 20) - 1).unwrap_or(0);
    Duration::from_secs((10u64 << exp).min(3600))
}

/// Records a failure: retries later if attempts are left (and `retry` is true), else gives up.
pub async fn fail(pool: &PgPool, job: &Job, error: &str, retry: bool) -> Result<(), Error> {
    if retry && job.attempt < job.max_attempts {
        sqlx::query!(
            "UPDATE jobs SET status = 'queued', run_at = now() + make_interval(secs => $2),
                    last_error = $3, lease_until = NULL, locked_by = NULL
             WHERE id = $1",
            job.id,
            backoff(job.attempt).as_secs_f64(),
            error,
        )
        .execute(pool)
        .await?;
    } else {
        sqlx::query!(
            "UPDATE jobs SET status = 'failed', finished_at = now(), last_error = $2, lease_until = NULL
             WHERE id = $1",
            job.id,
            error,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Puts jobs whose worker died back in the queue (or fails them if out of attempts).
/// Returns how many were touched.
pub async fn reap_expired(pool: &PgPool) -> Result<u64, Error> {
    let done = sqlx::query!(
        "UPDATE jobs
         SET status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END,
             finished_at = CASE WHEN attempts >= max_attempts THEN now() END,
             last_error = 'worker stopped responding (lease expired)',
             lease_until = NULL, locked_by = NULL
         WHERE status = 'running' AND lease_until < now()"
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Deletes finished jobs older than `age`.
pub async fn prune_finished(pool: &PgPool, age: Duration) -> Result<u64, Error> {
    let done = sqlx::query!(
        "DELETE FROM jobs WHERE status IN ('done', 'failed') AND finished_at < now() - make_interval(secs => $1)",
        age.as_secs_f64()
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Seconds until the next queued job of `kinds` is due, if any. Lets an idle worker sleep
/// exactly until then.
pub async fn next_due_in(pool: &PgPool, kinds: &[String]) -> Result<Option<Duration>, Error> {
    let secs = sqlx::query_scalar!(
        r#"SELECT EXTRACT(EPOCH FROM (min(run_at) - now()))::float8 AS "secs"
           FROM jobs WHERE status = 'queued' AND kind = ANY($1)"#,
        kinds
    )
    .fetch_one(pool)
    .await?;
    Ok(secs.map(|s| Duration::from_secs_f64(s.max(0.0))))
}

#[cfg(test)]
mod tests;
