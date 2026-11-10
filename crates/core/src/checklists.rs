//! Checklists on cards: create, rename, delete; items: add, edit, tick, move, delete. Every
//! change is recorded on the card's board like other board changes.

use chrono::Utc;
use laneway_db::PgPool;
use laneway_db::checklists::{self as db, Checklist, Item};
use serde_json::json;
use sqlx::PgTransaction;
use uuid::Uuid;

use crate::Error;
use crate::accounts::Invalid;
use crate::boards::{Actor, name, position_at, record, to_json, writable_card};

/// The checklist and the board it belongs to, if the actor may change it.
async fn writable_checklist(
    tx: &mut PgTransaction<'_>,
    actor: Actor,
    id: Uuid,
) -> Result<(Checklist, Uuid), Error> {
    let checklist = db::get(&mut **tx, id).await?.ok_or(Error::NotFound)?;
    let card = writable_card(tx, actor, checklist.card_id).await?;
    Ok((checklist, card.board_id))
}

async fn writable_item(tx: &mut PgTransaction<'_>, actor: Actor, id: Uuid) -> Result<(Item, Uuid), Error> {
    let item = db::item(&mut **tx, id).await?.ok_or(Error::NotFound)?;
    let (_, board_id) = writable_checklist(tx, actor, item.checklist_id).await?;
    Ok((item, board_id))
}

/// Adds a checklist at the end of a card's checklists.
pub async fn create(pool: &PgPool, actor: Actor, card_id: Uuid, title: &str) -> Result<Checklist, Error> {
    let title = name(title, 200, Invalid::ChecklistTitle)?;
    let mut tx = pool.begin().await?;
    let card = writable_card(&mut tx, actor, card_id).await?;
    let existing = db::of_card(&mut *tx, card_id).await?;
    let keys: Vec<&str> = existing.iter().map(|c| c.position.as_str()).collect();
    let checklist = Checklist {
        id: laneway_db::new_id(),
        card_id,
        title,
        position: position_at(&keys, keys.len())?,
        created_at: Utc::now(),
    };
    db::upsert(&mut *tx, &checklist).await?;
    record(
        &mut tx,
        actor,
        card.board_id,
        "checklist.create",
        to_json(&checklist),
        json!({ "delete_checklist": checklist.id }),
    )
    .await?;
    tx.commit().await?;
    Ok(checklist)
}

pub async fn rename(pool: &PgPool, actor: Actor, id: Uuid, title: &str) -> Result<Checklist, Error> {
    let title = name(title, 200, Invalid::ChecklistTitle)?;
    let mut tx = pool.begin().await?;
    let (old, board_id) = writable_checklist(&mut tx, actor, id).await?;
    let new = Checklist { title, ..old.clone() };
    db::upsert(&mut *tx, &new).await?;
    let fields = |c: &Checklist| json!({ "id": c.id, "title": c.title });
    record(&mut tx, actor, board_id, "checklist.update", fields(&new), fields(&old)).await?;
    tx.commit().await?;
    Ok(new)
}

/// Deletes a checklist with its items. The undo info keeps both.
pub async fn delete(pool: &PgPool, actor: Actor, id: Uuid) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    let (old, board_id) = writable_checklist(&mut tx, actor, id).await?;
    let items = db::items(&mut *tx, id).await?;
    db::delete(&mut *tx, id).await?;
    record(
        &mut tx,
        actor,
        board_id,
        "checklist.delete",
        json!({ "id": id }),
        json!({ "restore_checklist": old, "items": items }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Adds an item at the end of a checklist.
pub async fn add_item(pool: &PgPool, actor: Actor, checklist_id: Uuid, text: &str) -> Result<Item, Error> {
    let text = name(text, 500, Invalid::ItemText)?;
    let mut tx = pool.begin().await?;
    let (_, board_id) = writable_checklist(&mut tx, actor, checklist_id).await?;
    let existing = db::items(&mut *tx, checklist_id).await?;
    let keys: Vec<&str> = existing.iter().map(|i| i.position.as_str()).collect();
    let item = Item {
        id: laneway_db::new_id(),
        checklist_id,
        text,
        done: false,
        position: position_at(&keys, keys.len())?,
        created_at: Utc::now(),
    };
    db::upsert_item(&mut *tx, &item).await?;
    record(
        &mut tx,
        actor,
        board_id,
        "checklist.item.create",
        to_json(&item),
        json!({ "delete_item": item.id }),
    )
    .await?;
    tx.commit().await?;
    Ok(item)
}

/// Changes an item's text and/or ticks it. `None` = leave as is.
pub async fn update_item(
    pool: &PgPool,
    actor: Actor,
    id: Uuid,
    text: Option<&str>,
    done: Option<bool>,
) -> Result<Item, Error> {
    let mut tx = pool.begin().await?;
    let (old, board_id) = writable_item(&mut tx, actor, id).await?;
    let mut new = old.clone();
    if let Some(t) = text {
        new.text = name(t, 500, Invalid::ItemText)?;
    }
    if let Some(d) = done {
        new.done = d;
    }
    db::upsert_item(&mut *tx, &new).await?;
    let fields = |i: &Item| json!({ "id": i.id, "text": i.text, "done": i.done });
    record(&mut tx, actor, board_id, "checklist.item.update", fields(&new), fields(&old)).await?;
    tx.commit().await?;
    Ok(new)
}

/// Moves an item to `index` (0 = top) within its checklist.
pub async fn move_item(pool: &PgPool, actor: Actor, id: Uuid, index: usize) -> Result<Item, Error> {
    let mut tx = pool.begin().await?;
    let (old, board_id) = writable_item(&mut tx, actor, id).await?;
    let others: Vec<Item> =
        db::items(&mut *tx, old.checklist_id).await?.into_iter().filter(|i| i.id != id).collect();
    let keys: Vec<&str> = others.iter().map(|i| i.position.as_str()).collect();
    let new = Item { position: position_at(&keys, index)?, ..old.clone() };
    db::upsert_item(&mut *tx, &new).await?;
    let fields = |i: &Item| json!({ "id": i.id, "position": i.position });
    record(&mut tx, actor, board_id, "checklist.item.move", fields(&new), fields(&old)).await?;
    tx.commit().await?;
    Ok(new)
}

pub async fn delete_item(pool: &PgPool, actor: Actor, id: Uuid) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    let (old, board_id) = writable_item(&mut tx, actor, id).await?;
    db::delete_item(&mut *tx, id).await?;
    record(
        &mut tx,
        actor,
        board_id,
        "checklist.item.delete",
        json!({ "id": id }),
        json!({ "restore_item": old }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests;
