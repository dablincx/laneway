//! End-to-end API tests: real router, real database, real cookies.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use laneway_db::PgPool;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::http::router;
use crate::http::tests::{PUBLIC_URL, state, web_dir};

struct Api {
    router: Router,
    cookie: Option<String>,
    _dir: tempfile::TempDir,
}

struct Reply {
    status: StatusCode,
    body: Value,
    set_cookie: Option<String>,
}

impl Api {
    fn new(pool: PgPool) -> Self {
        let dir = web_dir();
        Self { router: router(state(pool), dir.path()), cookie: None, _dir: dir }
    }

    async fn call(&mut self, method: &str, uri: &str, body: Option<Value>, origin: Option<&str>) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(cookie) = &self.cookie {
            req = req.header(header::COOKIE, cookie);
        }
        if let Some(origin) = origin {
            req = req.header(header::ORIGIN, origin);
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let response = self.router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let set_cookie = response.headers().get(header::SET_COOKIE).map(|v| v.to_str().unwrap().to_owned());
        if let Some(c) = &set_cookie {
            let pair = c.split(';').next().unwrap().to_owned();
            self.cookie = if pair.ends_with('=') { None } else { Some(pair) };
        }
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply { status, body, set_cookie }
    }

    async fn get(&mut self, uri: &str) -> Reply {
        self.call("GET", uri, None, None).await
    }

    async fn post(&mut self, uri: &str, body: Value) -> Reply {
        self.call("POST", uri, Some(body), Some(PUBLIC_URL)).await
    }
}

fn admin(token: &str) -> Value {
    json!({
        "setup_token": token,
        "email": "admin@example.com",
        "username": "admin",
        "name": "Admin",
        "password": "a long enough password",
    })
}

async fn set_up(pool: &PgPool, api: &mut Api) {
    let token = laneway_core::accounts::issue_setup_token(pool).await.unwrap().unwrap();
    let reply = api.post("/api/v1/setup", admin(&token)).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn setup_then_login_logout(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    assert_eq!(api.get("/api/v1/instance").await.body["needs_setup"], true);
    assert_eq!(api.get("/api/v1/me").await.status, StatusCode::UNAUTHORIZED);

    let token = laneway_core::accounts::issue_setup_token(&pool).await.unwrap().unwrap();
    let wrong = api.post("/api/v1/setup", admin("wrong-token")).await;
    assert_eq!(
        (wrong.status, wrong.body["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("invalid_setup_token"))
    );

    let reply = api.post("/api/v1/setup", admin(&token)).await;
    assert_eq!(reply.status, StatusCode::CREATED);
    assert_eq!(reply.body["instance_role"], "admin");
    let cookie = reply.set_cookie.unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"), "{cookie}");
    assert!(!cookie.contains("Secure"), "plain http instance: {cookie}");

    // Setup logged us in.
    assert_eq!(api.get("/api/v1/me").await.body["username"], "admin");
    assert_eq!(api.get("/api/v1/instance").await.body["needs_setup"], false);

    assert_eq!(api.post("/api/v1/auth/logout", json!({})).await.status, StatusCode::NO_CONTENT);
    assert!(api.cookie.is_none());
    assert_eq!(api.get("/api/v1/me").await.status, StatusCode::UNAUTHORIZED);

    let reply = api
        .post(
            "/api/v1/auth/login",
            json!({"login": "ADMIN@example.com", "password": "a long enough password"}),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(api.get("/api/v1/me").await.body["email"], "admin@example.com");
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn old_session_cookie_stops_working_after_logout(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    let stolen = api.cookie.clone();
    api.post("/api/v1/auth/logout", json!({})).await;
    api.cookie = stolen;
    assert_eq!(api.get("/api/v1/me").await.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn wrong_password_then_rate_limit(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    api.cookie = None;
    let bad = json!({"login": "admin", "password": "wrong password!!"});
    let reply = api.post("/api/v1/auth/login", bad.clone()).await;
    assert_eq!(
        (reply.status, reply.body["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("invalid_credentials"))
    );
    for _ in 0..9 {
        api.post("/api/v1/auth/login", bad.clone()).await;
    }
    let blocked =
        api.post("/api/v1/auth/login", json!({"login": "admin", "password": "a long enough password"})).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn cross_site_posts_with_the_cookie_are_blocked(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    let evil = api.call("POST", "/api/v1/auth/logout", Some(json!({})), Some("https://evil.example")).await;
    assert_eq!(evil.status, StatusCode::FORBIDDEN);
    let no_origin = api.call("POST", "/api/v1/auth/logout", Some(json!({})), None).await;
    assert_eq!(no_origin.status, StatusCode::FORBIDDEN);
    // Still logged in.
    assert_eq!(api.get("/api/v1/me").await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn validation_errors_have_stable_codes(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    let token = laneway_core::accounts::issue_setup_token(&pool).await.unwrap().unwrap();
    let mut body = admin(&token);
    body["password"] = json!("short");
    let reply = api.post("/api/v1/setup", body).await;
    assert_eq!(
        (reply.status, reply.body["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("password_too_short"))
    );
    let mut body = admin(&token);
    body["email"] = json!("not-an-email");
    assert_eq!(api.post("/api/v1/setup", body).await.body["error"]["code"], "invalid_email");
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn boards_columns_cards_over_http(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;

    let created = api
        .post(
            "/api/v1/boards",
            json!({"name": "Home", "columns": [{"name": "To do"}, {"name": "Done", "is_done": true}]}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let board_id = created.body["board"]["id"].as_str().unwrap().to_owned();
    let todo = created.body["columns"][0]["id"].as_str().unwrap().to_owned();
    let done = created.body["columns"][1]["id"].as_str().unwrap().to_owned();

    let card = api.post(&format!("/api/v1/columns/{todo}/cards"), json!({"title": "Buy milk"})).await;
    assert_eq!(card.status, StatusCode::CREATED, "{}", card.body);
    let card_id = card.body["id"].as_str().unwrap().to_owned();

    let moved =
        api.post(&format!("/api/v1/cards/{card_id}/move"), json!({"column_id": done, "index": 0})).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert!(moved.body["completed_at"].is_string());

    let patched = api
        .call(
            "PATCH",
            &format!("/api/v1/cards/{card_id}"),
            Some(json!({"title": "Buy oat milk"})),
            Some(PUBLIC_URL),
        )
        .await;
    assert_eq!(patched.body["title"], "Buy oat milk");

    let view = api.get(&format!("/api/v1/boards/{board_id}")).await;
    assert_eq!(view.body["cards"][0]["column_id"], done.as_str());
    assert_eq!(api.get("/api/v1/boards").await.body.as_array().unwrap().len(), 1);

    let not_empty = api.call("DELETE", &format!("/api/v1/columns/{done}"), None, Some(PUBLIC_URL)).await;
    assert_eq!(
        (not_empty.status, not_empty.body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("column_not_empty"))
    );

    let history = api.get(&format!("/api/v1/boards/{board_id}/history?limit=2")).await;
    let types: Vec<&str> =
        history.body.as_array().unwrap().iter().map(|a| a["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["card.update", "card.move"]);

    // Logged out: nothing.
    api.cookie = None;
    assert_eq!(api.get(&format!("/api/v1/boards/{board_id}")).await.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn invalid_board_input_and_unknown_ids(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    let bad = api.post("/api/v1/boards", json!({"name": "   "})).await;
    assert_eq!(
        (bad.status, bad.body["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("invalid_board_name"))
    );
    let missing = api.get(&format!("/api/v1/boards/{}", laneway_db::new_id())).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

/// Behind a reverse proxy without trusted proxy headers every request has the proxy's address.
/// Failed logins for one name must not lock everybody else out.
#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn failed_logins_dont_lock_out_other_users_behind_a_proxy(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    api.cookie = None;
    let proxy = std::net::SocketAddr::from(([172, 18, 0, 2], 50000));

    let login = |api: &Api, login: &str, password: &str| {
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, PUBLIC_URL)
            .body(Body::from(json!({"login": login, "password": password}).to_string()))
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(proxy));
        api.router.clone().oneshot(req)
    };
    for _ in 0..12 {
        login(&api, "someone-else", "wrong password!!").await.unwrap();
    }
    let response = login(&api, "admin", "a long enough password").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// Bad requests get our error format with a stable code, not axum's plain-text rejections.
#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn malformed_requests_use_the_error_format(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;

    let not_a_uuid = api.get("/api/v1/boards/not-a-uuid").await;
    assert_eq!(
        (not_a_uuid.status, not_a_uuid.body["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_request"))
    );

    let missing_field = api.post("/api/v1/boards", json!({"description": "no name"})).await;
    assert_eq!(
        (missing_field.status, missing_field.body["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("invalid_request"))
    );

    // Broken JSON and a missing content type, sent by hand.
    for (content_type, body, status, code) in [
        (Some("application/json"), "{not json", StatusCode::BAD_REQUEST, "invalid_request"),
        (None, "{}", StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type"),
    ] {
        let mut req =
            Request::builder().method("POST").uri("/api/v1/boards").header(header::ORIGIN, PUBLIC_URL);
        req = req.header(header::COOKIE, api.cookie.clone().unwrap());
        if let Some(ct) = content_type {
            req = req.header(header::CONTENT_TYPE, ct);
        }
        let response = api.router.clone().oneshot(req.body(Body::from(body)).unwrap()).await.unwrap();
        assert_eq!(response.status(), status);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).expect("error body is JSON");
        assert_eq!(json["error"]["code"], code);
    }
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn card_details_patch_set_keep_clear(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    let created = api.post("/api/v1/boards", json!({"name": "Home", "columns": [{"name": "To do"}]})).await;
    let todo = created.body["columns"][0]["id"].as_str().unwrap().to_owned();
    let card = api.post(&format!("/api/v1/columns/{todo}/cards"), json!({"title": "Taxes"})).await;
    let uri = format!("/api/v1/cards/{}", card.body["id"].as_str().unwrap());
    assert!(card.body["priority"].is_null() && card.body["due_at"].is_null());

    let set = json!({
        "priority": "urgent",
        "start_at": "2026-03-01T09:00:00Z",
        "due_at": "2026-03-10T17:00:00Z",
        "estimate": 3,
    });
    let reply = api.call("PATCH", &uri, Some(set), Some(PUBLIC_URL)).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(reply.body["priority"], "urgent");
    assert_eq!(reply.body["due_at"], "2026-03-10T17:00:00Z");
    assert_eq!(reply.body["estimate"], 3.0);

    // Left out = kept, null = cleared.
    let reply =
        api.call("PATCH", &uri, Some(json!({"title": "Taxes!", "estimate": null})), Some(PUBLIC_URL)).await;
    assert_eq!(reply.body["priority"], "urgent");
    assert!(reply.body["estimate"].is_null(), "{}", reply.body);

    let backwards = json!({"start_at": "2026-04-01T00:00:00Z"});
    let reply = api.call("PATCH", &uri, Some(backwards), Some(PUBLIC_URL)).await;
    assert_eq!(
        (reply.status, reply.body["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("invalid_dates"))
    );
    let reply = api.call("PATCH", &uri, Some(json!({"estimate": -1})), Some(PUBLIC_URL)).await;
    assert_eq!(reply.body["error"]["code"], "invalid_estimate");
    // An unknown priority doesn't fit the request schema at all.
    let reply = api.call("PATCH", &uri, Some(json!({"priority": "whenever"})), Some(PUBLIC_URL)).await;
    assert_eq!(
        (reply.status, reply.body["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("invalid_request"))
    );
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn labels_over_http(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    let created = api.post("/api/v1/boards", json!({"name": "Home", "columns": [{"name": "To do"}]})).await;
    let board_id = created.body["board"]["id"].as_str().unwrap().to_owned();
    let todo = created.body["columns"][0]["id"].as_str().unwrap().to_owned();
    let card = api.post(&format!("/api/v1/columns/{todo}/cards"), json!({"title": "Taxes"})).await;
    let card_id = card.body["id"].as_str().unwrap().to_owned();

    let label = api
        .post(&format!("/api/v1/boards/{board_id}/labels"), json!({"name": "money", "color": "green"}))
        .await;
    assert_eq!(label.status, StatusCode::CREATED, "{}", label.body);
    let label_id = label.body["id"].as_str().unwrap().to_owned();
    let taken = api
        .post(&format!("/api/v1/boards/{board_id}/labels"), json!({"name": "Money", "color": "red"}))
        .await;
    assert_eq!(
        (taken.status, taken.body["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("label_taken"))
    );
    let bad =
        api.post(&format!("/api/v1/boards/{board_id}/labels"), json!({"name": "x", "color": "beige"})).await;
    assert_eq!(bad.body["error"]["code"], "invalid_request");

    let set = api
        .call(
            "PUT",
            &format!("/api/v1/cards/{card_id}/labels"),
            Some(json!({"label_ids": [label_id]})),
            Some(PUBLIC_URL),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.body);
    assert_eq!(set.body["label_ids"][0], label_id.as_str());

    let view = api.get(&format!("/api/v1/boards/{board_id}")).await;
    assert_eq!(view.body["labels"][0]["color"], "green");
    assert_eq!(view.body["card_labels"][0], json!({"card_id": card_id, "label_id": label_id}));

    let patched = api
        .call(
            "PATCH",
            &format!("/api/v1/labels/{label_id}"),
            Some(json!({"color": "teal"})),
            Some(PUBLIC_URL),
        )
        .await;
    assert_eq!(
        (patched.body["name"].as_str(), patched.body["color"].as_str()),
        (Some("money"), Some("teal"))
    );
    let deleted = api.call("DELETE", &format!("/api/v1/labels/{label_id}"), None, Some(PUBLIC_URL)).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let view = api.get(&format!("/api/v1/boards/{board_id}")).await;
    assert_eq!(view.body["card_labels"], json!([]));
}

#[sqlx::test(migrator = "laneway_db::MIGRATOR")]
async fn checklists_over_http(pool: PgPool) {
    let mut api = Api::new(pool.clone());
    set_up(&pool, &mut api).await;
    let created = api.post("/api/v1/boards", json!({"name": "Home", "columns": [{"name": "To do"}]})).await;
    let board_id = created.body["board"]["id"].as_str().unwrap().to_owned();
    let todo = created.body["columns"][0]["id"].as_str().unwrap().to_owned();
    let card = api.post(&format!("/api/v1/columns/{todo}/cards"), json!({"title": "Trip"})).await;
    let card_id = card.body["id"].as_str().unwrap().to_owned();

    let list = api.post(&format!("/api/v1/cards/{card_id}/checklists"), json!({"title": "Packing"})).await;
    assert_eq!(list.status, StatusCode::CREATED, "{}", list.body);
    let list_id = list.body["id"].as_str().unwrap().to_owned();
    let mut ids = Vec::new();
    for text in ["socks", "passport"] {
        let item = api.post(&format!("/api/v1/checklists/{list_id}/items"), json!({"text": text})).await;
        assert_eq!(item.status, StatusCode::CREATED, "{}", item.body);
        ids.push(item.body["id"].as_str().unwrap().to_owned());
    }
    let ticked = api
        .call(
            "PATCH",
            &format!("/api/v1/checklist-items/{}", ids[1]),
            Some(json!({"done": true})),
            Some(PUBLIC_URL),
        )
        .await;
    assert_eq!((ticked.body["done"].as_bool(), ticked.body["text"].as_str()), (Some(true), Some("passport")));
    let moved = api.post(&format!("/api/v1/checklist-items/{}/move", ids[1]), json!({"index": 0})).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);

    let view = api.get(&format!("/api/v1/boards/{board_id}")).await;
    assert_eq!(view.body["checklists"][0]["title"], "Packing");
    let texts: Vec<&str> = view.body["checklist_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["passport", "socks"]);

    let bad = api.post(&format!("/api/v1/checklists/{list_id}/items"), json!({"text": ""})).await;
    assert_eq!(bad.body["error"]["code"], "invalid_item_text");
    let gone = api.call("DELETE", &format!("/api/v1/checklists/{list_id}"), None, Some(PUBLIC_URL)).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let view = api.get(&format!("/api/v1/boards/{board_id}")).await;
    assert_eq!(
        (view.body["checklists"].clone(), view.body["checklist_items"].clone()),
        (json!([]), json!([]))
    );
}
