use laneway_db::PgPool;

use super::*;
use crate::accounts::{NewAccount, ensure_initial_admin};
use crate::boards::{self, NewColumn};

async fn user(pool: &PgPool, username: &str) -> Actor {
    let account = NewAccount {
        email: format!("{username}@example.com"),
        username: username.into(),
        name: username.into(),
        password: "a long enough password".into(),
    };
    let id = if let Some(u) = ensure_initial_admin(pool, &account).await.unwrap() {
        u.id
    } else {
        let mut tx = pool.begin().await.unwrap();
        let u = crate::accounts::tests_support::create_plain_user(&mut tx, &account).await;
        tx.commit().await.unwrap();
        u
    };
    Actor::User(id)
}

/// A board with one column and one card; returns (board id, card id).
async fn card(pool: &PgPool, actor: Actor) -> (Uuid, Uuid) {
    let view =
        boards::create(pool, actor, "Home", "", true, &[NewColumn { name: "To do".into(), is_done: false }])
            .await
            .unwrap();
    let card = boards::create_card(pool, actor, view.columns[0].id, "trip", "").await.unwrap();
    (view.board.id, card.id)
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn checklists_and_items(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let (board_id, card_id) = card(&pool, alex).await;
    let packing = create(&pool, alex, card_id, " Packing ").await.unwrap();
    let todo = create(&pool, alex, card_id, "Before leaving").await.unwrap();
    assert_eq!(packing.title, "Packing");
    assert!(packing.position < todo.position, "new checklists go last");

    let socks = add_item(&pool, alex, packing.id, "socks").await.unwrap();
    let passport = add_item(&pool, alex, packing.id, "passport").await.unwrap();
    let charger = add_item(&pool, alex, packing.id, "charger").await.unwrap();
    update_item(&pool, alex, passport.id, None, Some(true)).await.unwrap();
    let renamed = update_item(&pool, alex, socks.id, Some("warm socks"), None).await.unwrap();
    assert_eq!((renamed.text.as_str(), renamed.done), ("warm socks", false));
    move_item(&pool, alex, charger.id, 0).await.unwrap();

    let view = boards::get(&pool, alex, board_id).await.unwrap();
    assert_eq!(
        view.checklists.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
        ["Packing", "Before leaving"]
    );
    let items: Vec<(&str, bool)> = view
        .checklist_items
        .iter()
        .filter(|i| i.checklist_id == packing.id)
        .map(|i| (i.text.as_str(), i.done))
        .collect();
    assert_eq!(items, [("charger", false), ("warm socks", false), ("passport", true)]);

    rename(&pool, alex, todo.id, "Before we go").await.unwrap();
    delete_item(&pool, alex, charger.id).await.unwrap();
    // Deleting a checklist deletes its items; the undo info keeps them.
    delete(&pool, alex, packing.id).await.unwrap();
    let view = boards::get(&pool, alex, board_id).await.unwrap();
    assert_eq!(view.checklists.len(), 1);
    assert_eq!(view.checklist_items, []);
    let last = &boards::history(&pool, alex, board_id, 1).await.unwrap()[0];
    assert_eq!(last.kind, "checklist.delete");
    assert_eq!(last.inverse["items"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn checklist_validation_and_access(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let sam = user(&pool, "sam").await;
    let (_, card_id) = card(&pool, alex).await;
    assert!(matches!(create(&pool, alex, card_id, "  ").await, Err(Error::Invalid(Invalid::ChecklistTitle))));
    let list = create(&pool, alex, card_id, "list").await.unwrap();
    assert!(matches!(add_item(&pool, alex, list.id, "").await, Err(Error::Invalid(Invalid::ItemText))));
    assert!(matches!(
        add_item(&pool, alex, list.id, &"x".repeat(501)).await,
        Err(Error::Invalid(Invalid::ItemText))
    ));
    let item = add_item(&pool, alex, list.id, "one").await.unwrap();

    // Someone else's card: everything is "not found".
    assert!(matches!(create(&pool, sam, card_id, "mine").await, Err(Error::NotFound)));
    assert!(matches!(rename(&pool, sam, list.id, "x").await, Err(Error::NotFound)));
    assert!(matches!(add_item(&pool, sam, list.id, "x").await, Err(Error::NotFound)));
    assert!(matches!(update_item(&pool, sam, item.id, None, Some(true)).await, Err(Error::NotFound)));
    assert!(matches!(move_item(&pool, sam, item.id, 0).await, Err(Error::NotFound)));
    assert!(matches!(delete_item(&pool, sam, item.id).await, Err(Error::NotFound)));
    assert!(matches!(delete(&pool, sam, list.id).await, Err(Error::NotFound)));
}
