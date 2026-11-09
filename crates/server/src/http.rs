//! HTTP server: API, API docs, health checks and the static web UI.

use std::net::SocketAddr;
use std::path::Path;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::routing::get;
use axum::{Json, Router, middleware};
use laneway_config::{Config, Url};
use laneway_db::PgPool;
use tokio::sync::watch;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use utoipa_scalar::{Scalar, Servable};

use crate::api;
use crate::api::error::ApiError;
use crate::api::limit::LoginLimiter;

/// Where the built web UI lives inside the Docker image.
pub const WEB_DIRECTORY: &str = "/app/web";

/// Shared by all handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub public_url: Url,
    pub trust_proxy_headers: bool,
    pub login_limiter: LoginLimiter,
}

impl AppState {
    pub fn new(config: &Config, pool: PgPool) -> Self {
        Self {
            pool,
            public_url: config.public_url.clone(),
            trust_proxy_headers: config.trust_proxy_headers,
            login_limiter: LoginLimiter::default(),
        }
    }
}

pub async fn serve(config: Config, pool: PgPool, mut stop: watch::Receiver<bool>) -> Result<()> {
    // Always all interfaces: laneway runs in a container, the port mapping decides exposure.
    let addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    let listener =
        tokio::net::TcpListener::bind(addr).await.with_context(|| format!("can't listen on {addr}"))?;
    tracing::info!("listening on http://{addr}");
    let app = router(AppState::new(&config, pool), Path::new(WEB_DIRECTORY));
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            let _ = stop.wait_for(|stopped| *stopped).await;
        })
        .await?;
    Ok(())
}

pub fn router(state: AppState, web_directory: &Path) -> Router {
    let (api_router, spec) = api::split();
    let spec_for_docs = spec.clone();
    let api_router = api_router
        .route("/api/v1/openapi.json", get(move || async move { Json(spec.clone()) }))
        .route(
            "/api/{*rest}",
            get(api_not_found).post(api_not_found).put(api_not_found).delete(api_not_found),
        )
        .layer(middleware::from_fn_with_state(state.clone(), api::session::csrf));

    // Unknown non-API paths fall back to index.html so the web UI's router can handle them.
    let web = ServeDir::new(web_directory).fallback(ServeFile::new(web_directory.join("index.html")));
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .merge(api_router)
        .merge(Scalar::with_url("/api/docs", spec_for_docs))
        .fallback_service(web)
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Content Security Policy for the web UI: only our own origin. Inline styles are allowed
/// because Radix sets `style` attributes; inline scripts are not.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
    style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; \
    object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

/// Headers that make the browser stricter: no framing (clickjacking), no MIME sniffing, no
/// referrer to other sites, and a CSP. The API docs page loads Scalar from a CDN, so it gets
/// everything but the CSP.
async fn security_headers(
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let is_docs = request.uri().path().starts_with("/api/docs");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    let defaults = [
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "same-origin"),
    ];
    for (name, value) in defaults {
        headers.entry(name).or_insert(HeaderValue::from_static(value));
    }
    if !is_docs {
        headers
            .entry(header::CONTENT_SECURITY_POLICY)
            .or_insert(HeaderValue::from_static(CONTENT_SECURITY_POLICY));
    }
    response
}

async fn api_not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "not_found", "no such API endpoint")
}

async fn healthz() -> &'static str {
    "ok"
}

/// Ready = can serve traffic, i.e. the database answers.
async fn readyz(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match laneway_db::ping(&state.pool).await {
        Ok(()) => (StatusCode::OK, "ok"),
        Err(e) => {
            tracing::warn!("not ready: {e}");
            (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
        }
    }
}

#[cfg(test)]
pub mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    pub const PUBLIC_URL: &str = "http://laneway.test";

    pub fn state(pool: PgPool) -> AppState {
        AppState {
            pool,
            public_url: Url::parse(PUBLIC_URL).unwrap(),
            trust_proxy_headers: false,
            login_limiter: LoginLimiter::default(),
        }
    }

    async fn get(router: Router, uri: &str) -> (StatusCode, String) {
        let response = router.oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    /// A pool that never connects, for routes that don't touch the database.
    fn no_db() -> AppState {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_millis(200))
            .connect_lazy("postgres://nobody@127.0.0.1:1/none")
            .unwrap();
        state(pool)
    }

    fn app(dir: &tempfile::TempDir) -> Router {
        router(no_db(), dir.path())
    }

    pub fn web_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<p>laneway</p>").unwrap();
        std::fs::write(dir.path().join("app.js"), "console.log(1)").unwrap();
        dir
    }

    #[tokio::test]
    async fn health_needs_no_database() {
        let dir = web_dir();
        assert_eq!(get(app(&dir), "/healthz").await, (StatusCode::OK, "ok".into()));
    }

    #[tokio::test]
    async fn not_ready_without_database() {
        let dir = web_dir();
        assert_eq!(get(app(&dir), "/readyz").await.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[sqlx::test(migrations = false)]
    async fn ready_with_database(pool: PgPool) {
        let dir = web_dir();
        assert_eq!(get(router(state(pool), dir.path()), "/readyz").await, (StatusCode::OK, "ok".into()));
    }

    async fn headers_of(router: Router, uri: &str) -> axum::http::HeaderMap {
        router.oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap().headers().clone()
    }

    #[tokio::test]
    async fn security_headers_on_every_response() {
        let dir = web_dir();
        for uri in ["/", "/boards/x", "/app.js", "/healthz", "/api/v1/nope", "/api/v1/openapi.json"] {
            let h = headers_of(app(&dir), uri).await;
            assert_eq!(h["x-frame-options"], "DENY", "{uri}");
            assert_eq!(h["x-content-type-options"], "nosniff", "{uri}");
            assert_eq!(h["referrer-policy"], "same-origin", "{uri}");
            let csp = h["content-security-policy"].to_str().unwrap();
            assert!(
                csp.contains("frame-ancestors 'none'") && csp.contains("script-src 'self'"),
                "{uri}: {csp}"
            );
        }
        // The docs page loads its script from a CDN: no CSP there, the rest still applies.
        let docs = headers_of(app(&dir), "/api/docs").await;
        assert_eq!(docs["x-frame-options"], "DENY");
        assert!(docs.get("content-security-policy").is_none());
    }

    #[tokio::test]
    async fn web_ui_and_spa_fallback() {
        let dir = web_dir();
        assert_eq!(get(app(&dir), "/app.js").await.1, "console.log(1)");
        assert_eq!(get(app(&dir), "/boards/some-board").await.1, "<p>laneway</p>");
    }

    #[tokio::test]
    async fn unknown_api_paths_are_json_404s_not_the_web_ui() {
        let dir = web_dir();
        let (status, body) = get(app(&dir), "/api/v1/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("\"not_found\""), "{body}");
    }

    #[tokio::test]
    async fn openapi_spec_and_docs_page() {
        let dir = web_dir();
        let (status, body) = get(app(&dir), "/api/v1/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        let spec: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(spec["paths"]["/api/v1/auth/login"]["post"].is_object(), "{body}");
        assert_eq!(get(app(&dir), "/api/docs").await.0, StatusCode::OK);
    }
}
