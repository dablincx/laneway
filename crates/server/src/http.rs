//! HTTP server: API, health checks and the static web UI.

use std::net::SocketAddr;
use std::path::Path;

use anyhow::{Context, Result};
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use laneway_config::Config;
use laneway_db::PgPool;
use serde::Serialize;
use tokio::sync::watch;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// Where the built web UI lives inside the Docker image.
pub const WEB_DIRECTORY: &str = "/app/web";

/// Shared by all handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: PgPool,
}

pub async fn serve(config: Config, pool: PgPool, mut stop: watch::Receiver<bool>) -> Result<()> {
    // Always all interfaces: laneway runs in a container, the port mapping decides exposure.
    let addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    let listener =
        tokio::net::TcpListener::bind(addr).await.with_context(|| format!("can't listen on {addr}"))?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, router(AppState { pool }, Path::new(WEB_DIRECTORY)))
        .with_graceful_shutdown(async move {
            let _ = stop.wait_for(|stopped| *stopped).await;
        })
        .await?;
    Ok(())
}

pub fn router(state: AppState, web_directory: &Path) -> Router {
    // Unknown paths fall back to index.html so the web UI's router can handle them.
    let web = ServeDir::new(web_directory).fallback(ServeFile::new(web_directory.join("index.html")));
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .nest("/api/v1", api_v1())
        .fallback_service(web)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn api_v1() -> Router<AppState> {
    Router::new().route("/instance", get(instance))
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

#[derive(Debug, Serialize)]
struct Instance {
    name: &'static str,
    version: &'static str,
}

async fn instance() -> Json<Instance> {
    Json(Instance { name: "laneway", version: env!("CARGO_PKG_VERSION") })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

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
        AppState { pool }
    }

    fn app(dir: &tempfile::TempDir) -> Router {
        router(no_db(), dir.path())
    }

    fn web_dir() -> tempfile::TempDir {
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
        let router = router(AppState { pool }, dir.path());
        assert_eq!(get(router, "/readyz").await, (StatusCode::OK, "ok".into()));
    }

    #[tokio::test]
    async fn instance_info() {
        let dir = web_dir();
        let (status, body) = get(app(&dir), "/api/v1/instance").await;
        assert_eq!(status, StatusCode::OK);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["name"], "laneway");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn web_ui_and_spa_fallback() {
        let dir = web_dir();
        assert_eq!(get(app(&dir), "/app.js").await.1, "console.log(1)");
        assert_eq!(get(app(&dir), "/boards/some-board").await.1, "<p>laneway</p>");
    }
}
