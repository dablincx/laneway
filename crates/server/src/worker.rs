//! Background worker: runs the job queue (`laneway-jobs`).

use anyhow::Result;
use laneway_config::Config;
use laneway_db::PgPool;
use laneway_jobs::{Registry, Worker};
use tokio::sync::watch;

/// Every job kind laneway knows. Features register their handlers here.
pub fn registry() -> Registry {
    Registry::new()
}

pub async fn run(config: Config, pool: PgPool, stop: watch::Receiver<bool>) -> Result<()> {
    let concurrency = usize::try_from(config.worker_concurrency).unwrap_or(1);
    Worker::new(pool, registry()).concurrency(concurrency).run(stop).await?;
    Ok(())
}
