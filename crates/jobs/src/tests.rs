use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;
use sqlx::PgPool;
use tokio::sync::watch;

use super::*;

const LEASE: Duration = Duration::from_secs(60);

fn kinds(k: &[&str]) -> Vec<String> {
    k.iter().map(|s| (*s).to_owned()).collect()
}

async fn status(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar!("SELECT status FROM jobs WHERE id = $1", id).fetch_one(pool).await.unwrap()
}

/// Moves a job's `run_at` into the past, standing in for a fake clock.
async fn make_due(pool: &PgPool, id: Uuid) {
    sqlx::query!("UPDATE jobs SET run_at = now() - interval '1 second' WHERE id = $1", id)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn enqueue_claim_complete(pool: PgPool) {
    let id = enqueue(&pool, NewJob::new("email.send", &json!({"to": "a"})).unwrap()).await.unwrap();
    let job = claim(&pool, "w1", &kinds(&["email.send"]), LEASE).await.unwrap().unwrap();
    assert_eq!((job.id, job.attempt, job.payload["to"].as_str()), (id, 1, Some("a")));
    assert_eq!(status(&pool, id).await, "running");
    assert!(claim(&pool, "w2", &kinds(&["email.send"]), LEASE).await.unwrap().is_none());
    complete(&pool, id).await.unwrap();
    assert_eq!(status(&pool, id).await, "done");
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn only_claims_registered_kinds(pool: PgPool) {
    enqueue(&pool, NewJob::new("other", &json!({})).unwrap()).await.unwrap();
    assert!(claim(&pool, "w", &kinds(&["email.send"]), LEASE).await.unwrap().is_none());
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn delayed_jobs_wait(pool: PgPool) {
    let job = NewJob::new("k", &json!({})).unwrap().delay(Duration::from_secs(300));
    let id = enqueue(&pool, job).await.unwrap();
    assert!(claim(&pool, "w", &kinds(&["k"]), LEASE).await.unwrap().is_none());
    let due = next_due_in(&pool, &kinds(&["k"])).await.unwrap().unwrap();
    assert!(due > Duration::from_secs(290) && due <= Duration::from_secs(300), "{due:?}");
    make_due(&pool, id).await;
    assert!(claim(&pool, "w", &kinds(&["k"]), LEASE).await.unwrap().is_some());
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn replace_key_resets_the_timer(pool: PgPool) {
    let first =
        NewJob::new("git.push", &json!({"n": 1})).unwrap().replace("board-1").delay(Duration::from_secs(60));
    let a = enqueue(&pool, first).await.unwrap();
    let second =
        NewJob::new("git.push", &json!({"n": 2})).unwrap().replace("board-1").delay(Duration::from_secs(300));
    let b = enqueue(&pool, second).await.unwrap();
    assert_eq!(a, b, "replaced, not added");
    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM jobs"#).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    let due = next_due_in(&pool, &kinds(&["git.push"])).await.unwrap().unwrap();
    assert!(due > Duration::from_secs(290), "timer was reset: {due:?}");

    // A different key or a running job doesn't get replaced.
    let other = NewJob::new("git.push", &json!({})).unwrap().replace("board-2");
    assert_ne!(enqueue(&pool, other).await.unwrap(), a);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn replace_key_does_not_touch_running_jobs(pool: PgPool) {
    let a = enqueue(&pool, NewJob::new("git.push", &json!({})).unwrap().replace("b")).await.unwrap();
    claim(&pool, "w", &kinds(&["git.push"]), LEASE).await.unwrap().unwrap();
    let b = enqueue(&pool, NewJob::new("git.push", &json!({})).unwrap().replace("b")).await.unwrap();
    assert_ne!(a, b);
    assert_eq!(status(&pool, a).await, "running");
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn serial_jobs_run_one_at_a_time_in_order(pool: PgPool) {
    let k = kinds(&["git.commit"]);
    let first = enqueue(&pool, NewJob::new("git.commit", &json!(1)).unwrap().serial("board")).await.unwrap();
    let second = enqueue(&pool, NewJob::new("git.commit", &json!(2)).unwrap().serial("board")).await.unwrap();
    let other = enqueue(&pool, NewJob::new("git.commit", &json!(3)).unwrap().serial("other")).await.unwrap();

    let a = claim(&pool, "w1", &k, LEASE).await.unwrap().unwrap();
    assert_eq!(a.id, first);
    // "board" is busy, but "other" isn't.
    let b = claim(&pool, "w2", &k, LEASE).await.unwrap().unwrap();
    assert_eq!(b.id, other);
    assert!(claim(&pool, "w3", &k, LEASE).await.unwrap().is_none());

    complete(&pool, first).await.unwrap();
    assert_eq!(claim(&pool, "w3", &k, LEASE).await.unwrap().unwrap().id, second);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn a_retrying_serial_job_blocks_the_ones_behind_it(pool: PgPool) {
    let k = kinds(&["git.commit"]);
    let first = enqueue(&pool, NewJob::new("git.commit", &json!(1)).unwrap().serial("board")).await.unwrap();
    enqueue(&pool, NewJob::new("git.commit", &json!(2)).unwrap().serial("board")).await.unwrap();
    let job = claim(&pool, "w", &k, LEASE).await.unwrap().unwrap();
    fail(&pool, &job, "boom", true).await.unwrap();
    // first waits for its retry; second must not overtake it.
    assert!(claim(&pool, "w", &k, LEASE).await.unwrap().is_none());
    make_due(&pool, first).await;
    assert_eq!(claim(&pool, "w", &k, LEASE).await.unwrap().unwrap().id, first);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn retries_with_backoff_then_gives_up(pool: PgPool) {
    let k = kinds(&["k"]);
    let id = enqueue(&pool, NewJob::new("k", &json!({})).unwrap().max_attempts(2)).await.unwrap();
    let job = claim(&pool, "w", &k, LEASE).await.unwrap().unwrap();
    fail(&pool, &job, "first", true).await.unwrap();
    assert_eq!(status(&pool, id).await, "queued");
    let due = next_due_in(&pool, &k).await.unwrap().unwrap();
    assert!(due > Duration::from_secs(5) && due <= backoff(1), "{due:?}");

    make_due(&pool, id).await;
    let job = claim(&pool, "w", &k, LEASE).await.unwrap().unwrap();
    assert_eq!(job.attempt, 2);
    fail(&pool, &job, "second", true).await.unwrap();
    assert_eq!(status(&pool, id).await, "failed");
    let error =
        sqlx::query_scalar!("SELECT last_error FROM jobs WHERE id = $1", id).fetch_one(&pool).await.unwrap();
    assert_eq!(error.as_deref(), Some("second"));
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn permanent_failures_skip_retries(pool: PgPool) {
    let id = enqueue(&pool, NewJob::new("k", &json!({})).unwrap()).await.unwrap();
    let job = claim(&pool, "w", &kinds(&["k"]), LEASE).await.unwrap().unwrap();
    fail(&pool, &job, "bad payload", false).await.unwrap();
    assert_eq!(status(&pool, id).await, "failed");
}

#[test]
fn backoff_grows_and_caps() {
    assert_eq!(backoff(1), Duration::from_secs(10));
    assert_eq!(backoff(2), Duration::from_secs(20));
    assert_eq!(backoff(4), Duration::from_secs(80));
    assert_eq!(backoff(50), Duration::from_secs(3600));
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn expired_leases_are_requeued(pool: PgPool) {
    let id = enqueue(&pool, NewJob::new("k", &json!({})).unwrap()).await.unwrap();
    claim(&pool, "dead-worker", &kinds(&["k"]), Duration::from_millis(1)).await.unwrap().unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(reap_expired(&pool).await.unwrap(), 1);
    assert_eq!(status(&pool, id).await, "queued");
    assert!(!renew(&pool, id, "dead-worker", LEASE).await.unwrap(), "lease is gone");
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn rolled_back_transaction_enqueues_nothing(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    enqueue(&mut *tx, NewJob::new("k", &json!({})).unwrap()).await.unwrap();
    tx.rollback().await.unwrap();
    assert!(claim(&pool, "w", &kinds(&["k"]), LEASE).await.unwrap().is_none());
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn prune_removes_old_finished_jobs(pool: PgPool) {
    let id = enqueue(&pool, NewJob::new("k", &json!({})).unwrap()).await.unwrap();
    complete(&pool, id).await.unwrap();
    sqlx::query!("UPDATE jobs SET finished_at = now() - interval '8 days'").execute(&pool).await.unwrap();
    enqueue(&pool, NewJob::new("k", &json!({})).unwrap()).await.unwrap();
    assert_eq!(prune_finished(&pool, Duration::from_secs(7 * 24 * 3600)).await.unwrap(), 1);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn worker_runs_jobs_end_to_end(pool: PgPool) {
    let runs = Arc::new(AtomicUsize::new(0));
    let counter = runs.clone();
    let registry = Registry::new()
        .handle("ok", move |ctx: JobContext| {
            let counter = counter.clone();
            async move {
                let n: usize = ctx.payload()?;
                counter.fetch_add(n, Ordering::SeqCst);
                Ok(())
            }
        })
        .handle("broken", |_ctx| async { Err(JobError::Permanent("nope".into())) });

    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = Worker::new(pool.clone(), registry).poll_interval(Duration::from_millis(50));
    let handle = tokio::spawn(worker.run(stop_rx));

    // Enqueued after the worker started: the NOTIFY (or polling) must pick them up.
    for n in [1, 2, 3] {
        enqueue(&pool, NewJob::new("ok", &n).unwrap()).await.unwrap();
    }
    let broken = enqueue(&pool, NewJob::new("broken", &json!({})).unwrap()).await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while runs.load(Ordering::SeqCst) < 6 || status(&pool, broken).await != "failed" {
        assert!(tokio::time::Instant::now() < deadline, "jobs didn't run in time");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    stop_tx.send(true).unwrap();
    handle.await.unwrap().unwrap();
    assert_eq!(runs.load(Ordering::SeqCst), 6);
}
