//! Board labels and which cards carry them.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Label {
    pub id: Uuid,
    pub board_id: Uuid,
    pub name: String,
    /// One of the palette names, see the `labels.color` check in the migration.
    pub color: String,
    pub created_at: DateTime<Utc>,
}

/// A label on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CardLabel {
    pub card_id: Uuid,
    pub label_id: Uuid,
}

/// A board's labels, by name.
pub async fn list(executor: impl PgExecutor<'_>, board_id: Uuid) -> sqlx::Result<Vec<Label>> {
    sqlx::query_as!(Label, "SELECT * FROM labels WHERE board_id = $1 ORDER BY lower(name), id", board_id)
        .fetch_all(executor)
        .await
}

pub async fn get(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<Option<Label>> {
    sqlx::query_as!(Label, "SELECT * FROM labels WHERE id = $1", id).fetch_optional(executor).await
}

pub async fn upsert(executor: impl PgExecutor<'_>, l: &Label) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO labels (id, board_id, name, color, created_at) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, color = EXCLUDED.color",
        l.id,
        l.board_id,
        l.name,
        l.color,
        l.created_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete(executor: impl PgExecutor<'_>, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM labels WHERE id = $1", id).execute(executor).await?;
    Ok(())
}

/// Every label on every card of a board.
pub async fn on_board(executor: impl PgExecutor<'_>, board_id: Uuid) -> sqlx::Result<Vec<CardLabel>> {
    sqlx::query_as!(
        CardLabel,
        "SELECT cl.card_id, cl.label_id FROM card_labels cl JOIN labels l ON l.id = cl.label_id
         WHERE l.board_id = $1 ORDER BY cl.card_id, lower(l.name)",
        board_id
    )
    .fetch_all(executor)
    .await
}

/// The labels on one card, by name.
pub async fn of_card(executor: impl PgExecutor<'_>, card_id: Uuid) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar!(
        "SELECT cl.label_id FROM card_labels cl JOIN labels l ON l.id = cl.label_id
         WHERE cl.card_id = $1 ORDER BY lower(l.name)",
        card_id
    )
    .fetch_all(executor)
    .await
}

/// The cards that carry a label.
pub async fn cards_with(executor: impl PgExecutor<'_>, label_id: Uuid) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar!("SELECT card_id FROM card_labels WHERE label_id = $1 ORDER BY card_id", label_id)
        .fetch_all(executor)
        .await
}

/// Replaces a card's labels.
pub async fn set_on_card(
    conn: &mut sqlx::PgConnection,
    card_id: Uuid,
    label_ids: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM card_labels WHERE card_id = $1", card_id).execute(&mut *conn).await?;
    sqlx::query!(
        "INSERT INTO card_labels (card_id, label_id) SELECT $1, unnest($2::uuid[]) ON CONFLICT DO NOTHING",
        card_id,
        label_ids,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Puts a label back on cards (undo of a label delete).
pub async fn add_to_cards(
    executor: impl PgExecutor<'_>,
    label_id: Uuid,
    card_ids: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO card_labels (card_id, label_id) SELECT unnest($1::uuid[]), $2 ON CONFLICT DO NOTHING",
        card_ids,
        label_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}
