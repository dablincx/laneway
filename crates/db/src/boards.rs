//! Boards, columns and cards.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Board {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub description: String,
    pub agent_access_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Column {
    pub id: Uuid,
    pub board_id: Uuid,
    pub name: String,
    pub position: String,
    pub wip_limit: Option<i32>,
    pub is_done: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    pub id: Uuid,
    pub board_id: Uuid,
    pub column_id: Uuid,
    pub position: String,
    pub title: String,
    pub description: String,
    pub created_by: Uuid,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// `low`, `medium`, `high` or `urgent`; `None` = no priority.
    pub priority: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub due_at: Option<DateTime<Utc>>,
    /// Story points or hours, whatever the board uses.
    pub estimate: Option<f64>,
}

// --- boards ---

pub async fn list_owned(executor: impl PgExecutor<'_>, owner_id: Uuid) -> sqlx::Result<Vec<Board>> {
    sqlx::query_as!(Board, "SELECT * FROM boards WHERE owner_id = $1 ORDER BY lower(name), id", owner_id)
        .fetch_all(executor)
        .await
}

pub async fn get(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<Board>> {
    sqlx::query_as!(Board, "SELECT * FROM boards WHERE id = $1", id).fetch_optional(executor).await
}

/// Locks the board row for the rest of the transaction, so changes to one board are serialized.
pub async fn get_for_update(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<Board>> {
    sqlx::query_as!(Board, "SELECT * FROM boards WHERE id = $1 FOR UPDATE", id).fetch_optional(executor).await
}

pub async fn insert(executor: impl PgExecutor<'_>, board: &Board) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO boards (id, workspace_id, owner_id, name, description, agent_access_enabled, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        board.id,
        board.workspace_id,
        board.owner_id,
        board.name,
        board.description,
        board.agent_access_enabled,
        board.created_at,
        board.updated_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn update(executor: impl PgExecutor<'_>, board: &Board) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE boards SET name = $2, description = $3, agent_access_enabled = $4, updated_at = now() WHERE id = $1",
        board.id,
        board.name,
        board.description,
        board.agent_access_enabled,
    )
    .execute(executor)
    .await?;
    Ok(())
}

// --- columns ---

pub async fn columns(executor: impl PgExecutor<'_>, board_id: Uuid) -> sqlx::Result<Vec<Column>> {
    sqlx::query_as!(Column, "SELECT * FROM columns WHERE board_id = $1 ORDER BY position, id", board_id)
        .fetch_all(executor)
        .await
}

pub async fn column(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<Column>> {
    sqlx::query_as!(Column, "SELECT * FROM columns WHERE id = $1", id).fetch_optional(executor).await
}

/// Inserts or fully overwrites a column (used for create, update, move and undo alike).
pub async fn upsert_column(executor: impl PgExecutor<'_>, c: &Column) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO columns (id, board_id, name, position, wip_limit, is_done, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, position = EXCLUDED.position,
             wip_limit = EXCLUDED.wip_limit, is_done = EXCLUDED.is_done, updated_at = now()",
        c.id,
        c.board_id,
        c.name,
        c.position,
        c.wip_limit,
        c.is_done,
        c.created_at,
        c.updated_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete_column(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM columns WHERE id = $1", id).execute(executor).await?;
    Ok(())
}

// --- cards ---

pub async fn cards(executor: impl PgExecutor<'_>, board_id: Uuid) -> sqlx::Result<Vec<Card>> {
    sqlx::query_as!(Card, "SELECT * FROM cards WHERE board_id = $1 ORDER BY position, id", board_id)
        .fetch_all(executor)
        .await
}

pub async fn cards_in_column(executor: impl PgExecutor<'_>, column_id: Uuid) -> sqlx::Result<Vec<Card>> {
    sqlx::query_as!(Card, "SELECT * FROM cards WHERE column_id = $1 ORDER BY position, id", column_id)
        .fetch_all(executor)
        .await
}

pub async fn card(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<Card>> {
    sqlx::query_as!(Card, "SELECT * FROM cards WHERE id = $1", id).fetch_optional(executor).await
}

/// Inserts or fully overwrites a card (used for create, update, move and undo alike).
pub async fn upsert_card(executor: impl PgExecutor<'_>, c: &Card) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO cards (id, board_id, column_id, position, title, description, created_by, completed_at,
                            created_at, updated_at, priority, start_at, due_at, estimate)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         ON CONFLICT (id) DO UPDATE SET column_id = EXCLUDED.column_id, position = EXCLUDED.position,
             title = EXCLUDED.title, description = EXCLUDED.description,
             completed_at = EXCLUDED.completed_at, priority = EXCLUDED.priority,
             start_at = EXCLUDED.start_at, due_at = EXCLUDED.due_at, estimate = EXCLUDED.estimate,
             updated_at = now()",
        c.id,
        c.board_id,
        c.column_id,
        c.position,
        c.title,
        c.description,
        c.created_by,
        c.completed_at,
        c.created_at,
        c.updated_at,
        c.priority,
        c.start_at,
        c.due_at,
        c.estimate,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete_card(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM cards WHERE id = $1", id).execute(executor).await?;
    Ok(())
}
