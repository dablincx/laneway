//! HTTP server: API, health checks and the static web UI.

use std::net::SocketAddr;
use std::path::Path;

use anyhow::{Context, Result};
use axum::Json;
use axum::Router;
use axum::routing::get;
use laneway_config::Config;
use serde::Serialize;
use tokio::sync::watch;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// Where the built web UI lives inside the Docker image.
pub const WEB_DIRECTORY: &str = "/app/web";

pub async fn serve(config: Config, mut stop: watch::Receiver<bool>) -> Result<()> {
    // Always all interfaces: laneway runs in a container, the port mapping decides exposure.
    let addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    let listener =
        tokio::net::TcpListener::bind(addr).await.with_context(|| format!("can't listen on {addr}"))?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, router(Path::new(WEB_DIRECTORY)))
        .with_graceful_shutdown(async move {
            let _ = stop.wait_for(|stopped| *stopped).await;
        })
        .await?;
    Ok(())
}

pub fn router(web_directory: &Path) -> Router {
    // Unknown paths fall back to index.html so the web UI's router can handle them.
    let web = ServeDir::new(web_directory).fallback(ServeFile::new(web_directory.join("index.html")));
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .nest("/api/v1", api_v1())
        .fallback_service(web)
        .layer(TraceLayer::new_for_http())
}

fn api_v1() -> Router {
    Router::new().route("/instance", get(instance))
}

async fn healthz() -> &'static str {
    "ok"
}

/// Ready = can serve traffic. Will check the DB + queue once they exist.
async fn readyz() -> &'static str {
    "ok"
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

    fn web_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<p>laneway</p>").unwrap();
        std::fs::write(dir.path().join("app.js"), "console.log(1)").unwrap();
        dir
    }

    #[tokio::test]
    async fn health() {
        let dir = web_dir();
        assert_eq!(get(router(dir.path()), "/healthz").await, (StatusCode::OK, "ok".into()));
        assert_eq!(get(router(dir.path()), "/readyz").await, (StatusCode::OK, "ok".into()));
    }

    #[tokio::test]
    async fn instance_info() {
        let dir = web_dir();
        let (status, body) = get(router(dir.path()), "/api/v1/instance").await;
        assert_eq!(status, StatusCode::OK);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["name"], "laneway");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn web_ui_and_spa_fallback() {
        let dir = web_dir();
        assert_eq!(get(router(dir.path()), "/app.js").await.1, "console.log(1)");
        assert_eq!(get(router(dir.path()), "/boards/some-board").await.1, "<p>laneway</p>");
    }
}
