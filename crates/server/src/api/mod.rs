//! `/api/v1`. Every route is registered through `OpenApiRouter`, so the OpenAPI spec (and the
//! TypeScript client generated from it) always matches the code.

pub mod error;
pub mod limit;
pub mod session;
#[cfg(test)]
mod tests;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use laneway_core::accounts::{self, NewAccount};
use laneway_db::users::User;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use self::error::{ApiError, ErrorBody};
use self::session::{COOKIE_NAME, Client, CurrentUser};
use crate::http::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "laneway API",
        description = "REST API of laneway. The web UI uses exactly this API.",
        license(name = "GPL-3.0-or-later", identifier = "GPL-3.0-or-later"),
    ),
    tags((name = "instance"), (name = "auth"))
)]
struct ApiDoc;

/// All `/api/v1` routes (mounted under `/api/v1`) plus their spec.
pub fn split() -> (axum::Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi()).nest("/api/v1", routes()).split_for_parts()
}

fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(instance))
        .routes(routes!(setup))
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
}

/// The OpenAPI document, with paths as served (under `/api/v1`).
pub fn openapi() -> utoipa::openapi::OpenApi {
    split().1
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Instance {
    name: &'static str,
    version: &'static str,
    /// `true` until the first admin exists: show the setup page.
    needs_setup: bool,
}

/// What this server is. Clients call this first.
#[utoipa::path(get, path = "/instance", tag = "instance", responses((status = OK, body = Instance)))]
async fn instance(State(state): State<AppState>) -> Result<Json<Instance>, ApiError> {
    Ok(Json(Instance {
        name: "laneway",
        version: env!("CARGO_PKG_VERSION"),
        needs_setup: accounts::needs_setup(&state.pool).await?,
    }))
}

/// The logged-in user.
#[derive(Debug, Serialize, ToSchema)]
pub struct Me {
    id: Uuid,
    email: String,
    username: String,
    name: String,
    locale: Option<String>,
    timezone: Option<String>,
    /// `admin` or `user`.
    instance_role: String,
}

impl From<User> for Me {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            email: u.email,
            username: u.username,
            name: u.name,
            locale: u.locale,
            timezone: u.timezone,
            instance_role: u.instance_role,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupRequest {
    /// One-time token printed to the server logs.
    setup_token: String,
    email: String,
    username: String,
    name: String,
    password: String,
}

/// Creates the first admin with the setup token from the logs, and logs them in.
#[utoipa::path(
    post, path = "/setup", tag = "auth", request_body = SetupRequest,
    responses(
        (status = CREATED, body = Me),
        (status = FORBIDDEN, description = "wrong setup token", body = ErrorBody),
        (status = CONFLICT, description = "setup already done", body = ErrorBody),
        (status = UNPROCESSABLE_ENTITY, description = "invalid input", body = ErrorBody),
    )
)]
async fn setup(
    State(state): State<AppState>,
    Client(client): Client,
    jar: CookieJar,
    Json(body): Json<SetupRequest>,
) -> Result<(StatusCode, CookieJar, Json<Me>), ApiError> {
    let account =
        NewAccount { email: body.email, username: body.username, name: body.name, password: body.password };
    let user = accounts::complete_setup(&state.pool, &body.setup_token, &account).await?;
    let session = accounts::start_session(&state.pool, user, &client).await?;
    let jar = jar.add(session::session_cookie(&state, session.token, session.expires_at));
    Ok((StatusCode::CREATED, jar, Json(session.user.into())))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Email or username.
    login: String,
    password: String,
}

/// Logs in with email or username + password. Sets the session cookie.
#[utoipa::path(
    post, path = "/auth/login", tag = "auth", request_body = LoginRequest,
    responses(
        (status = OK, body = Me),
        (status = UNAUTHORIZED, description = "wrong login or password", body = ErrorBody),
        (status = TOO_MANY_REQUESTS, description = "too many failed attempts", body = ErrorBody),
    )
)]
async fn login(
    State(state): State<AppState>,
    Client(client): Client,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> Result<(CookieJar, Json<Me>), ApiError> {
    let mut keys = vec![format!("login:{}", body.login.trim().to_lowercase())];
    if let Some(ip) = client.ip {
        keys.push(format!("ip:{ip}"));
    }
    if !state.login_limiter.allowed(&keys) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_attempts",
            "too many failed logins, try again in 15 minutes",
        ));
    }
    match accounts::login(&state.pool, &body.login, &body.password, &client).await {
        Ok(session) => {
            state.login_limiter.succeeded(&keys[..1]);
            let jar = jar.add(session::session_cookie(&state, session.token, session.expires_at));
            Ok((jar, Json(session.user.into())))
        }
        Err(e) => {
            if matches!(e, laneway_core::Error::InvalidCredentials) {
                state.login_limiter.failed(&keys);
            }
            Err(e.into())
        }
    }
}

/// Ends the current session and clears the cookie.
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = NO_CONTENT)))]
async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<(StatusCode, CookieJar), ApiError> {
    if let Some(cookie) = jar.get(COOKIE_NAME) {
        accounts::logout(&state.pool, cookie.value()).await?;
    }
    Ok((StatusCode::NO_CONTENT, jar.add(session::removal_cookie(&state))))
}

/// The logged-in user.
#[utoipa::path(
    get, path = "/me", tag = "auth",
    responses((status = OK, body = Me), (status = UNAUTHORIZED, body = ErrorBody))
)]
async fn me(current: CurrentUser) -> Json<Me> {
    Json(current.user.into())
}
