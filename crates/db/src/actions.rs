//! The action log: every change to a board, with who made it and how to undo it.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Action {
    pub id: Uuid,
    pub board_id: Uuid,
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub on_behalf_of: Option<Uuid>,
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: serde_json::Value,
    pub inverse: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub async fn insert(executor: impl PgExecutor<'_>, a: &Action) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO actions (id, board_id, actor_type, actor_id, on_behalf_of, type, payload, inverse, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        a.id,
        a.board_id,
        a.actor_type,
        a.actor_id,
        a.on_behalf_of,
        a.kind,
        a.payload,
        a.inverse,
        a.created_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Newest first.
pub async fn list(executor: impl PgExecutor<'_>, board_id: Uuid, limit: i64) -> sqlx::Result<Vec<Action>> {
    sqlx::query_as!(
        Action,
        r#"SELECT id, board_id, actor_type, actor_id, on_behalf_of, type AS "kind", payload, inverse, created_at
           FROM actions WHERE board_id = $1 ORDER BY id DESC LIMIT $2"#,
        board_id,
        limit
    )
    .fetch_all(executor)
    .await
}
