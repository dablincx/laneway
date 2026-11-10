//! laneway: database access. Postgres 18 only, plain SQL migrations in `migrations/`.

pub mod actions;
pub mod boards;
pub mod checklists;
pub mod labels;
pub mod sessions;
pub mod settings;
pub mod setup;
pub mod users;
pub mod workspaces;

use std::time::Duration;

pub use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

/// Oldest supported Postgres, as `server_version_num`.
pub const MIN_POSTGRES_VERSION: i32 = 180_000;

/// Migrations embedded in the binary.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't connect to the database: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("Postgres {found} is too old, laneway needs Postgres 18 or newer")]
    TooOld { found: String },
    #[error("database migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Connects, checks the server version and returns a pool.
pub async fn connect(url: &str, max_connections: u32) -> Result<PgPool, Error> {
    let options: PgConnectOptions = url.parse().map_err(Error::Connect)?;
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options.application_name("laneway"))
        .await
        .map_err(Error::Connect)?;
    check_version(&pool).await?;
    Ok(pool)
}

/// Refuses Postgres older than 18.
pub async fn check_version(pool: &PgPool) -> Result<(), Error> {
    let (num, text): (String, String) =
        sqlx::query_as("SELECT current_setting('server_version_num'), current_setting('server_version')")
            .fetch_one(pool)
            .await?;
    if num.parse::<i32>().unwrap_or(0) < MIN_POSTGRES_VERSION {
        return Err(Error::TooOld { found: text });
    }
    Ok(())
}

/// Runs pending migrations. sqlx holds a Postgres advisory lock while doing it, so several
/// containers starting at once don't race.
pub async fn migrate(pool: &PgPool) -> Result<(), Error> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// Cheap liveness query for `/readyz`.
pub async fn ping(pool: &PgPool) -> Result<(), Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// New time-ordered id. Every entity uses `UUIDv7`.
pub fn new_id() -> uuid::Uuid {
    uuid::Uuid::now_v7()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = false)]
    async fn version_check_passes_on_test_server(pool: PgPool) {
        check_version(&pool).await.unwrap();
    }

    #[sqlx::test]
    async fn migrations_apply_and_uuidv7_works(pool: PgPool) {
        let id: uuid::Uuid = sqlx::query_scalar("INSERT INTO jobs (kind) VALUES ('test') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(id.get_version_num(), 7);
        ping(&pool).await.unwrap();
    }

    #[test]
    fn ids_are_v7_and_ordered() {
        let a = new_id();
        let b = new_id();
        assert_eq!(a.get_version_num(), 7);
        assert!(a < b);
    }
}
