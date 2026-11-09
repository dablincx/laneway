//! Background worker: runs the job queue (`laneway-jobs`) and periodic housekeeping.

use std::time::Duration;

use anyhow::Result;
use laneway_config::Config;
use laneway_db::PgPool;
use laneway_jobs::{Registry, Worker};
use tokio::sync::watch;

/// How often housekeeping runs.
const HOUSEKEEPING_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Every job kind laneway knows. Features register their handlers here.
pub fn registry() -> Registry {
    Registry::new()
}

pub async fn run(config: Config, pool: PgPool, stop: watch::Receiver<bool>) -> Result<()> {
    let concurrency = usize::try_from(config.worker_concurrency).unwrap_or(1);
    let housekeeping = tokio::spawn(housekeeping_loop(pool.clone(), stop.clone()));
    let result = Worker::new(pool, registry()).concurrency(concurrency).run(stop).await;
    housekeeping.await?;
    result?;
    Ok(())
}

async fn housekeeping_loop(pool: PgPool, mut stop: watch::Receiver<bool>) {
    let mut tick = tokio::time::interval(HOUSEKEEPING_INTERVAL);
    while !*stop.borrow() {
        tokio::select! {
            _ = tick.tick() => housekeeping(&pool).await,
            changed = stop.changed() => if changed.is_err() { return },
        }
    }
}

/// Deletes data nobody needs anymore. Safe to run from several workers at once.
pub async fn housekeeping(pool: &PgPool) {
    match laneway_db::sessions::delete_expired(pool).await {
        Ok(0) => {}
        Ok(n) => tracing::info!(sessions = n, "deleted expired sessions"),
        Err(e) => tracing::error!("deleting expired sessions failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn housekeeping_deletes_only_expired_sessions(pool: PgPool) {
        let account = laneway_core::accounts::NewAccount {
            email: "a@example.com".into(),
            username: "alex".into(),
            name: "Alex".into(),
            password: "a long enough password".into(),
        };
        laneway_core::accounts::ensure_initial_admin(&pool, &account).await.unwrap();
        let client = laneway_core::accounts::ClientInfo::default();
        let old =
            laneway_core::accounts::login(&pool, "alex", "a long enough password", &client).await.unwrap();
        let live =
            laneway_core::accounts::login(&pool, "alex", "a long enough password", &client).await.unwrap();
        sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 day' WHERE id = $1")
            .bind(old.id)
            .execute(&pool)
            .await
            .unwrap();

        housekeeping(&pool).await;

        let left: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM sessions").fetch_all(&pool).await.unwrap();
        assert_eq!(left, [live.id]);
    }
}
