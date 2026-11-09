//! Raw access to the `settings` table. Keys are env var names. This module stores whatever it
//! is given: validation and encryption of secrets happen in `laneway-core::settings`.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

/// One row of the `settings` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    pub key: String,
    /// The value as stored: ciphertext when `encrypted` is set.
    pub value: String,
    pub encrypted: bool,
    pub updated_at: DateTime<Utc>,
    /// The user who saved it. `None` for the CLI and system changes.
    pub updated_by: Option<Uuid>,
}

/// Every stored setting, ordered by key.
pub async fn all(pool: &PgPool) -> Result<Vec<SettingRow>, Error> {
    let rows = sqlx::query_as!(
        SettingRow,
        "SELECT key, value, encrypted, updated_at, updated_by FROM settings ORDER BY key"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Inserts or replaces a setting.
pub async fn upsert(
    pool: &PgPool,
    key: &str,
    value: &str,
    encrypted: bool,
    updated_by: Option<Uuid>,
) -> Result<(), Error> {
    sqlx::query!(
        "INSERT INTO settings (key, value, encrypted, updated_at, updated_by)
         VALUES ($1, $2, $3, now(), $4)
         ON CONFLICT (key) DO UPDATE
         SET value = EXCLUDED.value, encrypted = EXCLUDED.encrypted,
             updated_at = EXCLUDED.updated_at, updated_by = EXCLUDED.updated_by",
        key,
        value,
        encrypted,
        updated_by,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes a setting. Returns `false` if it wasn't stored.
pub async fn delete(pool: &PgPool, key: &str) -> Result<bool, Error> {
    let result = sqlx::query!("DELETE FROM settings WHERE key = $1", key).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn upsert_list_and_delete(pool: PgPool) {
        assert_eq!(all(&pool).await.unwrap(), []);

        upsert(&pool, "LANEWAY_B", "1", false, None).await.unwrap();
        let user = crate::new_id();
        upsert(&pool, "LANEWAY_A", "enc:v1:x:y", true, Some(user)).await.unwrap();
        let rows = all(&pool).await.unwrap();
        assert_eq!(rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["LANEWAY_A", "LANEWAY_B"]);
        assert!(rows[0].encrypted);
        assert_eq!(rows[0].updated_by, Some(user));
        assert_eq!(rows[1].updated_by, None);

        // Replacing keeps one row and updates every column.
        let before = rows[1].updated_at;
        upsert(&pool, "LANEWAY_B", "2", false, Some(user)).await.unwrap();
        let rows = all(&pool).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].value, "2");
        assert_eq!(rows[1].updated_by, Some(user));
        assert!(rows[1].updated_at >= before);

        assert!(delete(&pool, "LANEWAY_B").await.unwrap());
        assert!(!delete(&pool, "LANEWAY_B").await.unwrap());
        assert_eq!(all(&pool).await.unwrap().len(), 1);
    }
}
