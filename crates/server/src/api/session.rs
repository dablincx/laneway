//! Session cookie, the `CurrentUser` extractor and the CSRF origin check.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::{HeaderMap, Method, header, request::Parts};
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use laneway_core::accounts::ClientInfo;
use laneway_db::users::User;

use super::error::ApiError;
use crate::http::AppState;

pub const COOKIE_NAME: &str = "laneway_session";

/// The logged-in user. Rejects with 401 if there's no live session.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user: User,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token =
            jar.get(COOKIE_NAME).map(|c| c.value().to_owned()).ok_or_else(ApiError::unauthenticated)?;
        let (_session_id, user) = laneway_core::accounts::authenticate(&state.pool, &token)
            .await?
            .ok_or_else(ApiError::unauthenticated)?;
        Ok(Self { user })
    }
}

/// The session cookie. HttpOnly so scripts can't read it, Secure when the instance runs on https.
pub fn session_cookie(
    state: &AppState,
    token: String,
    expires: chrono::DateTime<chrono::Utc>,
) -> Cookie<'static> {
    let expires = time_from_chrono(expires);
    Cookie::build((COOKIE_NAME, token))
        .http_only(true)
        .secure(state.public_url.scheme() == "https")
        .same_site(SameSite::Lax)
        .path("/")
        .expires(expires)
        .build()
}

pub fn removal_cookie(state: &AppState) -> Cookie<'static> {
    let mut cookie = session_cookie(state, String::new(), chrono::Utc::now());
    cookie.make_removal();
    cookie
}

fn time_from_chrono(at: chrono::DateTime<chrono::Utc>) -> axum_extra::extract::cookie::Expiration {
    let ts =
        time::OffsetDateTime::from_unix_timestamp(at.timestamp()).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    axum_extra::extract::cookie::Expiration::DateTime(ts)
}

/// User agent + IP of the caller, for the sessions list.
#[derive(Debug, Clone)]
pub struct Client(pub ClientInfo);

impl FromRequestParts<AppState> for Client {
    type Rejection = std::convert::Infallible;

    #[allow(clippy::unused_async_trait_impl)] // the trait wants an async fn
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
        Ok(Self(client_info(state, &parts.headers, peer)))
    }
}

/// Forwarded headers only count behind a trusted proxy.
fn client_info(state: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>) -> ClientInfo {
    let forwarded = state
        .trust_proxy_headers
        .then(|| {
            headers.get("x-forwarded-for")?.to_str().ok()?.split(',').next()?.trim().parse::<IpAddr>().ok()
        })
        .flatten();
    ClientInfo {
        user_agent: headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_owned),
        ip: forwarded.or(peer.map(|p| p.ip())),
    }
}

/// Blocks cross-site requests that change things: a request with the session cookie and an
/// unsafe method must come from our own origin. Browsers always send `Origin` on those.
pub async fn csrf(State(state): State<AppState>, request: Request, next: Next) -> Result<Response, ApiError> {
    let unsafe_method = !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let has_cookie = CookieJar::from_headers(request.headers()).get(COOKIE_NAME).is_some();
    if unsafe_method && has_cookie && !same_origin(&state, request.headers()) {
        return Err(ApiError::forbidden("cross-site request blocked"));
    }
    Ok(next.run(request).await)
}

fn same_origin(state: &AppState, headers: &HeaderMap) -> bool {
    let expected = state.public_url.origin().ascii_serialization();
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        return origin == expected;
    }
    // No Origin header: only allow when the browser says it's same-origin.
    headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("same-origin")
}
