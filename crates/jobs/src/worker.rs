//! The worker loop: claims jobs, runs their handlers, keeps leases alive, retries failures.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::{Error, Job, NOTIFY_CHANNEL};

/// What a handler gets.
#[derive(Debug, Clone)]
pub struct JobContext {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    /// 1 on the first run.
    pub attempt: i32,
    pub pool: PgPool,
}

impl JobContext {
    /// Deserializes the payload.
    pub fn payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, JobError> {
        serde_json::from_value(self.payload.clone())
            .map_err(|e| JobError::Permanent(format!("bad payload: {e}")))
    }
}

/// Why a job failed.
#[derive(Debug)]
pub enum JobError {
    /// Try again later (network error, lock held, ...).
    Retry(String),
    /// Retrying won't help (bad payload, entity deleted, ...).
    Permanent(String),
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobError::Retry(e) | JobError::Permanent(e) => f.write_str(e),
        }
    }
}

impl<E: std::error::Error> From<E> for JobError {
    fn from(e: E) -> Self {
        JobError::Retry(e.to_string())
    }
}

type BoxFuture = Pin<Box<dyn Future<Output = Result<(), JobError>> + Send>>;

/// A job handler.
pub type Handler = Arc<dyn Fn(JobContext) -> BoxFuture + Send + Sync>;

/// Handlers by job kind.
#[derive(Clone, Default)]
pub struct Registry {
    handlers: HashMap<String, Handler>,
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry").field("kinds", &self.kinds()).finish()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the handler for a job kind.
    #[must_use]
    pub fn handle<F, Fut>(mut self, kind: impl Into<String>, handler: F) -> Self
    where
        F: Fn(JobContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), JobError>> + Send + 'static,
    {
        self.handlers.insert(kind.into(), Arc::new(move |ctx| Box::pin(handler(ctx))));
        self
    }

    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.handlers.keys().cloned().collect();
        kinds.sort();
        kinds
    }
}

/// Runs jobs until told to stop.
#[derive(Debug)]
pub struct Worker {
    pool: PgPool,
    registry: Registry,
    id: String,
    concurrency: usize,
    lease: Duration,
    poll_interval: Duration,
}

impl Worker {
    pub fn new(pool: PgPool, registry: Registry) -> Self {
        Self {
            pool,
            registry,
            id: format!("worker-{}", laneway_db::new_id()),
            concurrency: 4,
            lease: Duration::from_secs(60),
            poll_interval: Duration::from_secs(10),
        }
    }

    /// How many jobs run at the same time.
    #[must_use]
    pub fn concurrency(mut self, jobs: usize) -> Self {
        self.concurrency = jobs.max(1);
        self
    }

    /// How long a job stays claimed without a renewal. Renewed every third of it.
    #[must_use]
    pub fn lease(mut self, lease: Duration) -> Self {
        self.lease = lease;
        self
    }

    /// Fallback polling when no notification arrives.
    #[must_use]
    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Main loop. Finishes running jobs before returning when `stop` turns true.
    pub async fn run(self, mut stop: watch::Receiver<bool>) -> Result<(), Error> {
        let kinds = self.registry.kinds();
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(NOTIFY_CHANNEL).await?;
        let permits = Arc::new(Semaphore::new(self.concurrency));
        let mut running = JoinSet::new();
        let mut last_maintenance = tokio::time::Instant::now() - Duration::from_secs(3600);
        tracing::info!(worker = %self.id, ?kinds, "worker started");

        loop {
            if *stop.borrow() {
                break;
            }
            if last_maintenance.elapsed() >= Duration::from_secs(30) {
                last_maintenance = tokio::time::Instant::now();
                match crate::reap_expired(&self.pool).await {
                    Ok(0) => {}
                    Ok(n) => tracing::warn!(jobs = n, "requeued jobs whose worker stopped responding"),
                    Err(e) => tracing::error!("reaping expired jobs failed: {e}"),
                }
                if let Err(e) = crate::prune_finished(&self.pool, Duration::from_secs(7 * 24 * 3600)).await {
                    tracing::error!("pruning finished jobs failed: {e}");
                }
            }

            // Claim as many jobs as there are free slots.
            let mut claimed_any = false;
            while let Ok(permit) = permits.clone().try_acquire_owned() {
                match crate::claim(&self.pool, &self.id, &kinds, self.lease).await {
                    Ok(Some(job)) => {
                        claimed_any = true;
                        let task = self.run_job(job);
                        running.spawn(async move {
                            task.await;
                            drop(permit);
                        });
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("claiming a job failed: {e}");
                        break;
                    }
                }
            }
            while running.try_join_next().is_some() {}
            if claimed_any {
                continue;
            }

            let wait = match crate::next_due_in(&self.pool, &kinds).await {
                Ok(Some(due)) => due.min(self.poll_interval),
                _ => self.poll_interval,
            };
            tokio::select! {
                _ = stop.changed() => {}
                _ = listener.recv() => {}
                () = tokio::time::sleep(wait) => {}
                Some(_) = running.join_next(), if !running.is_empty() => {}
            }
        }

        tracing::info!(running = running.len(), "worker stopping, waiting for running jobs");
        while running.join_next().await.is_some() {}
        Ok(())
    }

    fn run_job(&self, job: Job) -> impl Future<Output = ()> + Send + 'static {
        let pool = self.pool.clone();
        let worker_id = self.id.clone();
        let lease = self.lease;
        let handler = self.registry.handlers.get(&job.kind).cloned();
        async move {
            let Some(handler) = handler else {
                let _ = crate::fail(&pool, &job, "no handler for this job kind", false).await;
                return;
            };
            let ctx = JobContext {
                id: job.id,
                kind: job.kind.clone(),
                payload: job.payload.clone(),
                attempt: job.attempt,
                pool: pool.clone(),
            };
            let work = handler(ctx);
            tokio::pin!(work);
            let mut renew = tokio::time::interval(lease / 3);
            renew.tick().await;
            let outcome = loop {
                tokio::select! {
                    outcome = &mut work => break outcome,
                    _ = renew.tick() => {
                        if let Err(e) = crate::renew(&pool, job.id, &worker_id, lease).await {
                            tracing::warn!(id = %job.id, "renewing job lease failed: {e}");
                        }
                    }
                }
            };

            let result = match &outcome {
                Ok(()) => crate::complete(&pool, job.id).await,
                Err(e) => {
                    let retry = matches!(e, JobError::Retry(_));
                    tracing::warn!(id = %job.id, kind = %job.kind, attempt = job.attempt, retry, "job failed: {e}");
                    crate::fail(&pool, &job, &e.to_string(), retry).await
                }
            };
            if let Err(e) = result {
                tracing::error!(id = %job.id, "saving job result failed: {e}");
            }
        }
    }
}
