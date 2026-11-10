//! Checklists on cards and their items.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Checklist {
    pub id: Uuid,
    pub card_id: Uuid,
    pub title: String,
    pub position: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub id: Uuid,
    pub checklist_id: Uuid,
    pub text: String,
    pub done: bool,
    pub position: String,
    pub created_at: DateTime<Utc>,
}

/// All checklists on a board's cards, in order per card.
pub async fn on_board(executor: impl PgExecutor<'_>, board_id: Uuid) -> sqlx::Result<Vec<Checklist>> {
    sqlx::query_as!(
        Checklist,
        "SELECT cl.* FROM checklists cl JOIN cards c ON c.id = cl.card_id
         WHERE c.board_id = $1 ORDER BY cl.card_id, cl.position, cl.id",
        board_id
    )
    .fetch_all(executor)
    .await
}

/// All items of all checklists on a board, in order per checklist.
pub async fn items_on_board(executor: impl PgExecutor<'_>, board_id: Uuid) -> sqlx::Result<Vec<Item>> {
    sqlx::query_as!(
        Item,
        "SELECT i.* FROM checklist_items i JOIN checklists cl ON cl.id = i.checklist_id
         JOIN cards c ON c.id = cl.card_id
         WHERE c.board_id = $1 ORDER BY i.checklist_id, i.position, i.id",
        board_id
    )
    .fetch_all(executor)
    .await
}

pub async fn of_card(executor: impl PgExecutor<'_>, card_id: Uuid) -> sqlx::Result<Vec<Checklist>> {
    sqlx::query_as!(Checklist, "SELECT * FROM checklists WHERE card_id = $1 ORDER BY position, id", card_id)
        .fetch_all(executor)
        .await
}

pub async fn get(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<Checklist>> {
    sqlx::query_as!(Checklist, "SELECT * FROM checklists WHERE id = $1", id).fetch_optional(executor).await
}

pub async fn upsert(executor: impl PgExecutor<'_>, c: &Checklist) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO checklists (id, card_id, title, position, created_at) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, position = EXCLUDED.position",
        c.id,
        c.card_id,
        c.title,
        c.position,
        c.created_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM checklists WHERE id = $1", id).execute(executor).await?;
    Ok(())
}

pub async fn items(executor: impl PgExecutor<'_>, checklist_id: Uuid) -> sqlx::Result<Vec<Item>> {
    sqlx::query_as!(
        Item,
        "SELECT * FROM checklist_items WHERE checklist_id = $1 ORDER BY position, id",
        checklist_id
    )
    .fetch_all(executor)
    .await
}

pub async fn item(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<Item>> {
    sqlx::query_as!(Item, "SELECT * FROM checklist_items WHERE id = $1", id).fetch_optional(executor).await
}

pub async fn upsert_item(executor: impl PgExecutor<'_>, i: &Item) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO checklist_items (id, checklist_id, text, done, position, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (id) DO UPDATE SET text = EXCLUDED.text, done = EXCLUDED.done, position = EXCLUDED.position",
        i.id,
        i.checklist_id,
        i.text,
        i.done,
        i.position,
        i.created_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete_item(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM checklist_items WHERE id = $1", id).execute(executor).await?;
    Ok(())
}
