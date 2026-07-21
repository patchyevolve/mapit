//! Web server backend for the mapit codebase mapper.
//!
//! Serves the embedded React frontend (via `rust-embed`) and exposes:
//! - **REST API** (`/api/*`) — annotate, remap, graph queries, flaws, simulation
//! - **WebSocket** (`/api/events`) — real-time progress updates for long-running tasks
//!
//! The server is started by `mapit-cli` after structural mapping completes.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::RustEmbed;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::info;

pub mod api;
pub mod state;

#[derive(RustEmbed)]
#[folder = "embedded_dist"]
struct WebAssets;

/// Serve a single embedded file, with SPA fallback to index.html.
async fn serve_static(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() || path == "index.html" {
        "index.html"
    } else {
        path
    };

    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // SPA fallback: serve index.html for any non-file route
            match WebAssets::get("index.html") {
                Some(content) => Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data))
                    .unwrap(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

/// Try `preferred` port first; if busy, scan upwards until a free one is found.
pub async fn find_free_port(preferred: u16) -> Result<u16> {
    for port in preferred..=65535 {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                drop(listener);
                return Ok(port);
            }
            Err(_) => continue,
        }
    }
    anyhow::bail!("no free port found in range {preferred}..=65535")
}

/// Start the mapit HTTP server on `127.0.0.1:<port>`.
/// Blocks until the server shuts down (e.g., via Ctrl+C).
pub async fn serve(db_path: &Path, port: u16, project_root: Option<&Path>) -> Result<()> {
    let store = mapit_core::graph::store::GraphStore::open(db_path)
        .with_context(|| format!("opening graph store at {}", db_path.display()))?;
    let store = Arc::new(Mutex::new(store));

    let (ws_tx, _) = broadcast::channel(256);

    let root = project_root.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
    let mapit_dir = db_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".mapit"));

    let app_state = Arc::new(state::AppState {
        store,
        ws_tx: ws_tx.clone(),
        progress: Arc::new(Mutex::new(state::ProgressState::default())),
        project_root: root,
        mapit_dir,
        cancel_flag: Arc::new(AtomicBool::new(false)),
    });

    let ws_route = get(move |ws: axum::extract::ws::WebSocketUpgrade, State(state): State<Arc<state::AppState>>| async move {
        let rx = state.ws_tx.subscribe();
        ws.on_upgrade(move |socket| async move {
            crate::ws::handle_ws(socket, rx).await;
        })
    });

    let app = Router::new()
        .merge(api::routes())
        .route("/api/events", ws_route)
        .fallback(serve_static)
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    info!("Server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;

    axum::serve(listener, app)
        .await
        .context("server error")?;

    Ok(())
}

mod ws {
    use axum::extract::ws::{Message, WebSocket};
    use tokio::sync::broadcast;
    use tracing::warn;

    pub async fn handle_ws(mut ws: WebSocket, mut rx: broadcast::Receiver<String>) {
        loop {
            tokio::select! {
                msg = ws.recv() => {
                    match msg {
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            warn!("WS error: {e}");
                            break;
                        }
                    }
                }
                event = rx.recv() => {
                    match event {
                        Ok(payload) => {
                            if ws.send(Message::Text(payload.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("WS lagged behind {n} events");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }
}
