use laneway_db::PgPool;

use super::*;
use crate::accounts::{NewAccount, ensure_initial_admin};

async fn user(pool: &PgPool, username: &str) -> Actor {
    let account = NewAccount {
        email: format!("{username}@example.com"),
        username: username.into(),
        name: username.into(),
        password: "a long enough password".into(),
    };
    // First call creates an admin; later users go through the same insert path.
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

fn cols(names: &[(&str, bool)]) -> Vec<NewColumn> {
    names.iter().map(|(n, d)| NewColumn { name: (*n).into(), is_done: *d }).collect()
}

async fn board(pool: &PgPool, actor: Actor) -> BoardView {
    create(pool, actor, "Home", "", true, &cols(&[("To do", false), ("Doing", false), ("Done", true)]))
        .await
        .unwrap()
}

async fn action_types(pool: &PgPool, actor: Actor, board_id: Uuid) -> Vec<String> {
    let mut types: Vec<String> =
        history(pool, actor, board_id, 100).await.unwrap().into_iter().map(|a| a.kind).collect();
    types.reverse();
    types
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn create_board_with_columns_records_actions(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let view = board(&pool, alex).await;
    assert_eq!(view.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["To do", "Doing", "Done"]);
    assert!(view.columns[2].is_done);
    assert_eq!(list(&pool, alex).await.unwrap().len(), 1);
    assert_eq!(
        action_types(&pool, alex, view.board.id).await,
        ["board.create", "column.create", "column.create", "column.create"]
    );
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn cards_move_between_columns_and_get_completed(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let view = board(&pool, alex).await;
    let (todo, done) = (view.columns[0].id, view.columns[2].id);
    let a = create_card(&pool, alex, todo, "A", "").await.unwrap();
    let b = create_card(&pool, alex, todo, "B", "").await.unwrap();
    let c = create_card(&pool, alex, todo, "C", "").await.unwrap();

    // C to the top.
    move_card(&pool, alex, c.id, todo, 0).await.unwrap();
    let order = |v: &BoardView, col: Uuid| -> Vec<String> {
        v.cards.iter().filter(|x| x.column_id == col).map(|x| x.title.clone()).collect()
    };
    let v = get(&pool, alex, view.board.id).await.unwrap();
    assert_eq!(order(&v, todo), ["C", "A", "B"]);

    // A into done, between nothing: completed.
    let moved = move_card(&pool, alex, a.id, done, 5).await.unwrap();
    assert!(moved.completed_at.is_some());
    let back = move_card(&pool, alex, a.id, todo, 1).await.unwrap();
    assert!(back.completed_at.is_none());
    let v = get(&pool, alex, view.board.id).await.unwrap();
    assert_eq!(order(&v, todo), ["C", "A", "B"]);

    let updated = update_card(
        &pool,
        alex,
        b.id,
        CardChanges { title: Some(" B2 ".into()), description: Some("x".into()), ..Default::default() },
    )
    .await
    .unwrap();
    assert_eq!((updated.title.as_str(), updated.description.as_str()), ("B2", "x"));
    delete_card(&pool, alex, b.id).await.unwrap();
    assert_eq!(get(&pool, alex, view.board.id).await.unwrap().cards.len(), 2);

    let history = history(&pool, alex, view.board.id, 100).await.unwrap();
    let delete = history.iter().find(|a| a.kind == "card.delete").unwrap();
    assert_eq!(delete.inverse["restore_card"]["title"], "B2", "undo info keeps the whole card");
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn columns_reorder_and_only_empty_ones_can_be_deleted(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let view = board(&pool, alex).await;
    move_column(&pool, alex, view.columns[2].id, 0).await.unwrap();
    let names: Vec<String> =
        get(&pool, alex, view.board.id).await.unwrap().columns.into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["Done", "To do", "Doing"]);

    create_card(&pool, alex, view.columns[1].id, "card", "").await.unwrap();
    assert!(matches!(delete_column(&pool, alex, view.columns[1].id).await, Err(Error::ColumnNotEmpty)));
    delete_column(&pool, alex, view.columns[0].id).await.unwrap();

    let renamed = update_column(
        &pool,
        alex,
        view.columns[1].id,
        ColumnChanges { name: Some("Now".into()), is_done: None, wip_limit: Some(Some(3)) },
    )
    .await
    .unwrap();
    assert_eq!((renamed.name.as_str(), renamed.wip_limit), ("Now", Some(3)));
    let bad = ColumnChanges { wip_limit: Some(Some(0)), ..Default::default() };
    assert!(matches!(
        update_column(&pool, alex, view.columns[1].id, bad).await,
        Err(Error::Invalid(Invalid::WipLimit))
    ));
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn other_users_cannot_see_or_touch_a_board(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let sam = user(&pool, "sam").await;
    let view = board(&pool, alex).await;
    let card = create_card(&pool, alex, view.columns[0].id, "secret", "").await.unwrap();

    assert_eq!(list(&pool, sam).await.unwrap(), [] as [laneway_db::boards::Board; 0]);
    assert!(matches!(get(&pool, sam, view.board.id).await, Err(Error::NotFound)));
    assert!(matches!(create_card(&pool, sam, view.columns[0].id, "x", "").await, Err(Error::NotFound)));
    assert!(matches!(delete_card(&pool, sam, card.id).await, Err(Error::NotFound)));
    assert!(matches!(move_card(&pool, sam, card.id, view.columns[1].id, 0).await, Err(Error::NotFound)));
    assert!(matches!(history(&pool, sam, view.board.id, 10).await, Err(Error::NotFound)));

    // Cards can't be moved into another user's board either.
    let sams = board(&pool, sam).await;
    assert!(matches!(move_card(&pool, alex, card.id, sams.columns[0].id, 0).await, Err(Error::NotFound)));
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn validation(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    assert!(matches!(
        create(&pool, alex, "  ", "", true, &[]).await,
        Err(Error::Invalid(Invalid::BoardName))
    ));
    let view = board(&pool, alex).await;
    assert!(matches!(
        create_card(&pool, alex, view.columns[0].id, "", "").await,
        Err(Error::Invalid(Invalid::CardTitle))
    ));
    let long = "x".repeat(501);
    assert!(matches!(
        create_card(&pool, alex, view.columns[0].id, &long, "").await,
        Err(Error::Invalid(Invalid::CardTitle))
    ));
    // Nothing was recorded for the failed calls.
    assert_eq!(action_types(&pool, alex, view.board.id).await.len(), 4);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn card_details_set_keep_and_clear(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let view = board(&pool, alex).await;
    let card = create_card(&pool, alex, view.columns[0].id, "taxes", "").await.unwrap();
    assert_eq!((card.priority.clone(), card.due_at, card.estimate), (None, None, None));

    let start = Utc::now();
    let due = start + chrono::Duration::days(3);
    let set = CardChanges {
        priority: Some(Some("high".into())),
        start_at: Some(Some(start)),
        due_at: Some(Some(due)),
        estimate: Some(Some(2.5)),
        ..Default::default()
    };
    let card = update_card(&pool, alex, card.id, set).await.unwrap();
    assert_eq!(card.priority.as_deref(), Some("high"));
    assert_eq!(card.estimate, Some(2.5));
    // Postgres keeps microseconds.
    assert_eq!(card.due_at.map(|d| d.timestamp_micros()), Some(due.timestamp_micros()));

    // Leaving fields out keeps them; the stored card matches what update returned.
    let card = update_card(
        &pool,
        alex,
        card.id,
        CardChanges { title: Some("taxes 2026".into()), ..Default::default() },
    )
    .await
    .unwrap();
    assert_eq!((card.priority.as_deref(), card.estimate), (Some("high"), Some(2.5)));
    let stored =
        get(&pool, alex, view.board.id).await.unwrap().cards.into_iter().find(|c| c.id == card.id).unwrap();
    assert_eq!(stored.priority.as_deref(), Some("high"));
    assert!(stored.start_at.is_some() && stored.due_at.is_some());

    // `Some(None)` clears.
    let clear = CardChanges {
        priority: Some(None),
        start_at: Some(None),
        due_at: Some(None),
        estimate: Some(None),
        ..Default::default()
    };
    let card = update_card(&pool, alex, card.id, clear).await.unwrap();
    assert_eq!((card.priority, card.start_at, card.due_at, card.estimate), (None, None, None, None));

    // The action can undo it: the inverse has the old values.
    let history = history(&pool, alex, view.board.id, 1).await.unwrap();
    assert_eq!(history[0].inverse["priority"], "high");
    assert_eq!(history[0].inverse["estimate"], 2.5);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn card_details_validation(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let view = board(&pool, alex).await;
    let card = create_card(&pool, alex, view.columns[0].id, "c", "").await.unwrap();
    let try_update = |changes: CardChanges| update_card(&pool, alex, card.id, changes);

    let bad_priority = CardChanges { priority: Some(Some("whenever".into())), ..Default::default() };
    assert!(matches!(try_update(bad_priority).await, Err(Error::Invalid(Invalid::Priority))));
    for e in [-1.0, 100_001.0, f64::NAN, f64::INFINITY] {
        let bad = CardChanges { estimate: Some(Some(e)), ..Default::default() };
        assert!(matches!(try_update(bad).await, Err(Error::Invalid(Invalid::Estimate))), "{e}");
    }
    let now = Utc::now();
    let backwards = CardChanges {
        start_at: Some(Some(now)),
        due_at: Some(Some(now - chrono::Duration::hours(1))),
        ..Default::default()
    };
    assert!(matches!(try_update(backwards).await, Err(Error::Invalid(Invalid::Dates))));

    // A start date after an already stored due date is caught too.
    let due = CardChanges { due_at: Some(Some(now)), ..Default::default() };
    try_update(due).await.unwrap();
    let late_start =
        CardChanges { start_at: Some(Some(now + chrono::Duration::days(1))), ..Default::default() };
    assert!(matches!(try_update(late_start).await, Err(Error::Invalid(Invalid::Dates))));
    // Same moment is fine.
    let same = CardChanges { start_at: Some(Some(now)), ..Default::default() };
    try_update(same).await.unwrap();
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn labels_on_cards(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let view = board(&pool, alex).await;
    let card = create_card(&pool, alex, view.columns[0].id, "c", "").await.unwrap();
    let bug = create_label(&pool, alex, view.board.id, " bug ", "red").await.unwrap();
    let ui = create_label(&pool, alex, view.board.id, "UI", "blue").await.unwrap();
    assert_eq!(bug.name, "bug");

    // Names are unique per board, ignoring case.
    assert!(matches!(create_label(&pool, alex, view.board.id, "BUG", "green").await, Err(Error::LabelTaken)));
    assert!(matches!(update_label(&pool, alex, ui.id, Some("Bug"), None).await, Err(Error::LabelTaken)));
    assert!(matches!(
        create_label(&pool, alex, view.board.id, "x", "chartreuse").await,
        Err(Error::Invalid(Invalid::LabelColor))
    ));
    assert!(matches!(
        create_label(&pool, alex, view.board.id, &"x".repeat(51), "red").await,
        Err(Error::Invalid(Invalid::LabelName))
    ));

    let set = set_card_labels(&pool, alex, card.id, &[ui.id, bug.id]).await.unwrap();
    assert_eq!(set, [bug.id, ui.id], "sorted by name");
    let v = get(&pool, alex, view.board.id).await.unwrap();
    assert_eq!(v.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["bug", "UI"]);
    assert_eq!(v.card_labels.len(), 2);

    // Setting the same labels again records nothing new.
    let before = history(&pool, alex, view.board.id, 500).await.unwrap().len();
    set_card_labels(&pool, alex, card.id, &[bug.id, ui.id]).await.unwrap();
    assert_eq!(history(&pool, alex, view.board.id, 500).await.unwrap().len(), before);

    let renamed = update_label(&pool, alex, ui.id, Some("frontend"), Some("purple")).await.unwrap();
    assert_eq!((renamed.name.as_str(), renamed.color.as_str()), ("frontend", "purple"));

    // Deleting a label takes it off the card; the undo info knows which cards had it.
    delete_label(&pool, alex, bug.id).await.unwrap();
    let v = get(&pool, alex, view.board.id).await.unwrap();
    assert_eq!(v.card_labels.iter().map(|cl| cl.label_id).collect::<Vec<_>>(), [ui.id]);
    let last = &history(&pool, alex, view.board.id, 1).await.unwrap()[0];
    assert_eq!(last.kind, "label.delete");
    assert_eq!(last.inverse["on_cards"][0], card.id.to_string());
    assert_eq!(last.inverse["restore_label"]["name"], "bug");
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn labels_stay_on_their_board(pool: PgPool) {
    let alex = user(&pool, "alex").await;
    let sam = user(&pool, "sam").await;
    let alexs = board(&pool, alex).await;
    let other = board(&pool, alex).await;
    let card = create_card(&pool, alex, alexs.columns[0].id, "c", "").await.unwrap();
    let foreign = create_label(&pool, alex, other.board.id, "elsewhere", "gray").await.unwrap();
    // A label of another board can't go on this card.
    assert!(matches!(set_card_labels(&pool, alex, card.id, &[foreign.id]).await, Err(Error::NotFound)));
    // Other users can't see or touch labels.
    assert!(matches!(create_label(&pool, sam, alexs.board.id, "x", "red").await, Err(Error::NotFound)));
    assert!(matches!(update_label(&pool, sam, foreign.id, Some("y"), None).await, Err(Error::NotFound)));
    assert!(matches!(delete_label(&pool, sam, foreign.id).await, Err(Error::NotFound)));
    assert!(matches!(set_card_labels(&pool, sam, card.id, &[]).await, Err(Error::NotFound)));
}
