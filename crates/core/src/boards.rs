//! Boards, columns and cards. Every change runs in one transaction that also writes an
//! [action](laneway_db::actions) with an inverse, so history and revert (v0.4) can build on it.
//!
//! Access, for now: only the board's owner sees or changes it. Other users get "not found",
//! the same as for a board that doesn't exist. The permission resolver replaces this in v0.3.

use chrono::Utc;
use laneway_db::PgPool;
use laneway_db::actions::Action;
use laneway_db::boards::{self as db, Board, Card, Column};
use serde_json::{Value, json};
use sqlx::PgTransaction;
use uuid::Uuid;

use crate::accounts::Invalid;
use crate::{Error, position};

/// Who is acting. Agents and API keys join in v0.5.
#[derive(Debug, Clone, Copy)]
pub enum Actor {
    User(Uuid),
}

impl Actor {
    fn user_id(self) -> Uuid {
        match self {
            Actor::User(id) => id,
        }
    }
}

/// A board with everything on it, columns and cards in order.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BoardView {
    pub board: Board,
    pub columns: Vec<Column>,
    pub cards: Vec<Card>,
}

// --- validation ---

fn name(value: &str, max: usize, invalid: Invalid) -> Result<String, Error> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max {
        return Err(Error::Invalid(invalid));
    }
    Ok(trimmed.to_owned())
}

fn description(value: &str) -> Result<String, Error> {
    if value.chars().count() > 100_000 {
        return Err(Error::Invalid(Invalid::Description));
    }
    Ok(value.to_owned())
}

fn wip_limit(value: Option<i32>) -> Result<Option<i32>, Error> {
    match value {
        Some(n) if n < 1 => Err(Error::Invalid(Invalid::WipLimit)),
        other => Ok(other),
    }
}

// --- access + actions ---

/// Loads and locks a board the actor may change, or `NotFound`.
async fn writable_board(tx: &mut PgTransaction<'_>, actor: Actor, board_id: Uuid) -> Result<Board, Error> {
    let board = db::get_for_update(&mut **tx, board_id).await?.ok_or(Error::NotFound)?;
    if board.owner_id != actor.user_id() {
        return Err(Error::NotFound);
    }
    Ok(board)
}

async fn record(
    tx: &mut PgTransaction<'_>,
    actor: Actor,
    board_id: Uuid,
    kind: &str,
    payload: Value,
    inverse: Value,
) -> Result<(), Error> {
    let Actor::User(user_id) = actor;
    let action = Action {
        id: laneway_db::new_id(),
        board_id,
        actor_type: "user".into(),
        actor_id: Some(user_id),
        on_behalf_of: None,
        kind: kind.into(),
        payload,
        inverse,
        created_at: Utc::now(),
    };
    laneway_db::actions::insert(&mut **tx, &action).await?;
    // v0.4: enqueue the git commit job for this action here, in the same transaction.
    Ok(())
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Key for putting an item at `index` among `others` (already sorted, without the item).
fn position_at(others: &[&str], index: usize) -> Result<String, Error> {
    let index = index.min(others.len());
    let before = index.checked_sub(1).map(|i| others[i]);
    let after = others.get(index).copied();
    position::between(before, after).map_err(|_| Error::Invalid(Invalid::Position))
}

// --- boards ---

/// Boards the actor can see.
pub async fn list(pool: &PgPool, actor: Actor) -> Result<Vec<Board>, Error> {
    Ok(db::list_owned(pool, actor.user_id()).await?)
}

/// A new column for [`create`].
#[derive(Debug, Clone)]
pub struct NewColumn {
    pub name: String,
    pub is_done: bool,
}

/// Creates a board in the actor's personal workspace, with optional starting columns.
pub async fn create(
    pool: &PgPool,
    actor: Actor,
    board_name: &str,
    board_description: &str,
    agent_access_enabled: bool,
    columns: &[NewColumn],
) -> Result<BoardView, Error> {
    let board_name = name(board_name, 200, Invalid::BoardName)?;
    let board_description = description(board_description)?;
    let mut tx = pool.begin().await?;
    let workspace_id =
        laneway_db::workspaces::personal_id(&mut *tx, actor.user_id()).await?.ok_or(Error::NotFound)?;
    let now = Utc::now();
    let board = Board {
        id: laneway_db::new_id(),
        workspace_id,
        owner_id: actor.user_id(),
        name: board_name,
        description: board_description,
        agent_access_enabled,
        created_at: now,
        updated_at: now,
    };
    db::insert(&mut *tx, &board).await?;
    record(&mut tx, actor, board.id, "board.create", to_json(&board), json!({ "delete_board": board.id }))
        .await?;

    let mut created = Vec::new();
    for (new, key) in columns.iter().zip(position::spread(columns.len())) {
        let column = Column {
            id: laneway_db::new_id(),
            board_id: board.id,
            name: name(&new.name, 200, Invalid::ColumnName)?,
            position: key,
            wip_limit: None,
            is_done: new.is_done,
            created_at: now,
            updated_at: now,
        };
        db::upsert_column(&mut *tx, &column).await?;
        record(
            &mut tx,
            actor,
            board.id,
            "column.create",
            to_json(&column),
            json!({ "delete_column": column.id }),
        )
        .await?;
        created.push(column);
    }
    tx.commit().await?;
    Ok(BoardView { board, columns: created, cards: Vec::new() })
}

pub async fn get(pool: &PgPool, actor: Actor, board_id: Uuid) -> Result<BoardView, Error> {
    let board = db::get(pool, board_id).await?.ok_or(Error::NotFound)?;
    if board.owner_id != actor.user_id() {
        return Err(Error::NotFound);
    }
    let columns = db::columns(pool, board_id).await?;
    let cards = db::cards(pool, board_id).await?;
    Ok(BoardView { board, columns, cards })
}

/// Fields to change on a board. `None` = leave as is.
#[derive(Debug, Clone, Default)]
pub struct BoardChanges {
    pub name: Option<String>,
    pub description: Option<String>,
    pub agent_access_enabled: Option<bool>,
}

pub async fn update(
    pool: &PgPool,
    actor: Actor,
    board_id: Uuid,
    changes: BoardChanges,
) -> Result<Board, Error> {
    let mut tx = pool.begin().await?;
    let old = writable_board(&mut tx, actor, board_id).await?;
    let mut new = old.clone();
    if let Some(n) = &changes.name {
        new.name = name(n, 200, Invalid::BoardName)?;
    }
    if let Some(d) = &changes.description {
        new.description = description(d)?;
    }
    if let Some(a) = changes.agent_access_enabled {
        new.agent_access_enabled = a;
    }
    db::update(&mut *tx, &new).await?;
    let fields = |b: &Board| json!({ "name": b.name, "description": b.description, "agent_access_enabled": b.agent_access_enabled });
    record(&mut tx, actor, board_id, "board.update", fields(&new), fields(&old)).await?;
    tx.commit().await?;
    Ok(new)
}

// --- columns ---

/// Adds a column at the end of the board.
pub async fn create_column(
    pool: &PgPool,
    actor: Actor,
    board_id: Uuid,
    column_name: &str,
    is_done: bool,
    limit: Option<i32>,
) -> Result<Column, Error> {
    let column_name = name(column_name, 200, Invalid::ColumnName)?;
    let limit = wip_limit(limit)?;
    let mut tx = pool.begin().await?;
    writable_board(&mut tx, actor, board_id).await?;
    let existing = db::columns(&mut *tx, board_id).await?;
    let last = existing.last().map(|c| c.position.as_str());
    let now = Utc::now();
    let column = Column {
        id: laneway_db::new_id(),
        board_id,
        name: column_name,
        position: position::between(last, None).map_err(|_| Error::Invalid(Invalid::Position))?,
        wip_limit: limit,
        is_done,
        created_at: now,
        updated_at: now,
    };
    db::upsert_column(&mut *tx, &column).await?;
    record(
        &mut tx,
        actor,
        board_id,
        "column.create",
        to_json(&column),
        json!({ "delete_column": column.id }),
    )
    .await?;
    tx.commit().await?;
    Ok(column)
}

async fn writable_column(tx: &mut PgTransaction<'_>, actor: Actor, column_id: Uuid) -> Result<Column, Error> {
    let column = db::column(&mut **tx, column_id).await?.ok_or(Error::NotFound)?;
    writable_board(tx, actor, column.board_id).await?;
    // Re-read under the board lock.
    db::column(&mut **tx, column_id).await?.ok_or(Error::NotFound)
}

/// Fields to change on a column. `wip_limit: Some(None)` removes the limit.
#[derive(Debug, Clone, Default)]
pub struct ColumnChanges {
    pub name: Option<String>,
    pub is_done: Option<bool>,
    pub wip_limit: Option<Option<i32>>,
}

pub async fn update_column(
    pool: &PgPool,
    actor: Actor,
    column_id: Uuid,
    changes: ColumnChanges,
) -> Result<Column, Error> {
    let mut tx = pool.begin().await?;
    let old = writable_column(&mut tx, actor, column_id).await?;
    let mut new = old.clone();
    if let Some(n) = &changes.name {
        new.name = name(n, 200, Invalid::ColumnName)?;
    }
    if let Some(d) = changes.is_done {
        new.is_done = d;
    }
    if let Some(l) = changes.wip_limit {
        new.wip_limit = wip_limit(l)?;
    }
    db::upsert_column(&mut *tx, &new).await?;
    let fields =
        |c: &Column| json!({ "id": c.id, "name": c.name, "is_done": c.is_done, "wip_limit": c.wip_limit });
    record(&mut tx, actor, new.board_id, "column.update", fields(&new), fields(&old)).await?;
    tx.commit().await?;
    Ok(new)
}

/// Moves a column to `index` (0 = first) among the board's columns.
pub async fn move_column(
    pool: &PgPool,
    actor: Actor,
    column_id: Uuid,
    index: usize,
) -> Result<Column, Error> {
    let mut tx = pool.begin().await?;
    let old = writable_column(&mut tx, actor, column_id).await?;
    let others: Vec<Column> =
        db::columns(&mut *tx, old.board_id).await?.into_iter().filter(|c| c.id != column_id).collect();
    let keys: Vec<&str> = others.iter().map(|c| c.position.as_str()).collect();
    let mut new = old.clone();
    new.position = position_at(&keys, index)?;
    db::upsert_column(&mut *tx, &new).await?;
    record(
        &mut tx,
        actor,
        new.board_id,
        "column.move",
        json!({ "id": new.id, "position": new.position }),
        json!({ "id": old.id, "position": old.position }),
    )
    .await?;
    tx.commit().await?;
    Ok(new)
}

/// Deletes an empty column. Columns with cards must be emptied first.
pub async fn delete_column(pool: &PgPool, actor: Actor, column_id: Uuid) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    let old = writable_column(&mut tx, actor, column_id).await?;
    if !db::cards_in_column(&mut *tx, column_id).await?.is_empty() {
        return Err(Error::ColumnNotEmpty);
    }
    db::delete_column(&mut *tx, column_id).await?;
    record(
        &mut tx,
        actor,
        old.board_id,
        "column.delete",
        json!({ "id": old.id }),
        json!({ "restore_column": old }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// --- cards ---

/// Adds a card at the bottom of a column.
pub async fn create_card(
    pool: &PgPool,
    actor: Actor,
    column_id: Uuid,
    title: &str,
    card_description: &str,
) -> Result<Card, Error> {
    let title = name(title, 500, Invalid::CardTitle)?;
    let card_description = description(card_description)?;
    let mut tx = pool.begin().await?;
    let column = writable_column(&mut tx, actor, column_id).await?;
    let existing = db::cards_in_column(&mut *tx, column_id).await?;
    let last = existing.last().map(|c| c.position.as_str());
    let now = Utc::now();
    let card = Card {
        id: laneway_db::new_id(),
        board_id: column.board_id,
        column_id,
        position: position::between(last, None).map_err(|_| Error::Invalid(Invalid::Position))?,
        title,
        description: card_description,
        created_by: actor.user_id(),
        completed_at: column.is_done.then_some(now),
        created_at: now,
        updated_at: now,
    };
    db::upsert_card(&mut *tx, &card).await?;
    record(&mut tx, actor, card.board_id, "card.create", to_json(&card), json!({ "delete_card": card.id }))
        .await?;
    tx.commit().await?;
    Ok(card)
}

async fn writable_card(tx: &mut PgTransaction<'_>, actor: Actor, card_id: Uuid) -> Result<Card, Error> {
    let card = db::card(&mut **tx, card_id).await?.ok_or(Error::NotFound)?;
    writable_board(tx, actor, card.board_id).await?;
    db::card(&mut **tx, card_id).await?.ok_or(Error::NotFound)
}

/// Fields to change on a card. `None` = leave as is.
#[derive(Debug, Clone, Default)]
pub struct CardChanges {
    pub title: Option<String>,
    pub description: Option<String>,
}

pub async fn update_card(
    pool: &PgPool,
    actor: Actor,
    card_id: Uuid,
    changes: CardChanges,
) -> Result<Card, Error> {
    let mut tx = pool.begin().await?;
    let old = writable_card(&mut tx, actor, card_id).await?;
    let mut new = old.clone();
    if let Some(t) = &changes.title {
        new.title = name(t, 500, Invalid::CardTitle)?;
    }
    if let Some(d) = &changes.description {
        new.description = description(d)?;
    }
    db::upsert_card(&mut *tx, &new).await?;
    let fields = |c: &Card| json!({ "id": c.id, "title": c.title, "description": c.description });
    record(&mut tx, actor, new.board_id, "card.update", fields(&new), fields(&old)).await?;
    tx.commit().await?;
    Ok(new)
}

/// Moves a card to `index` (0 = top) in a column of the same board. Moving into a done column
/// marks it completed, moving out clears that.
pub async fn move_card(
    pool: &PgPool,
    actor: Actor,
    card_id: Uuid,
    column_id: Uuid,
    index: usize,
) -> Result<Card, Error> {
    let mut tx = pool.begin().await?;
    let old = writable_card(&mut tx, actor, card_id).await?;
    let target = db::column(&mut *tx, column_id).await?.ok_or(Error::NotFound)?;
    if target.board_id != old.board_id {
        return Err(Error::NotFound);
    }
    let others: Vec<Card> =
        db::cards_in_column(&mut *tx, column_id).await?.into_iter().filter(|c| c.id != card_id).collect();
    let keys: Vec<&str> = others.iter().map(|c| c.position.as_str()).collect();
    let mut new = old.clone();
    new.column_id = column_id;
    new.position = position_at(&keys, index)?;
    new.completed_at = match (target.is_done, old.completed_at) {
        (true, Some(at)) => Some(at),
        (true, None) => Some(Utc::now()),
        (false, _) => None,
    };
    db::upsert_card(&mut *tx, &new).await?;
    let fields = |c: &Card| json!({ "id": c.id, "column_id": c.column_id, "position": c.position, "completed_at": c.completed_at });
    record(&mut tx, actor, new.board_id, "card.move", fields(&new), fields(&old)).await?;
    tx.commit().await?;
    Ok(new)
}

/// Deletes a card for good. The trash bin (v0.2) replaces this with a soft delete.
pub async fn delete_card(pool: &PgPool, actor: Actor, card_id: Uuid) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    let old = writable_card(&mut tx, actor, card_id).await?;
    db::delete_card(&mut *tx, card_id).await?;
    record(
        &mut tx,
        actor,
        old.board_id,
        "card.delete",
        json!({ "id": old.id }),
        json!({ "restore_card": old }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Recent actions on a board, newest first.
pub async fn history(pool: &PgPool, actor: Actor, board_id: Uuid, limit: i64) -> Result<Vec<Action>, Error> {
    get(pool, actor, board_id).await?;
    Ok(laneway_db::actions::list(pool, board_id, limit.clamp(1, 500)).await?)
}

#[cfg(test)]
mod tests;
