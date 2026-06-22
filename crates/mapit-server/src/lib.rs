use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::{
    extract::State,
    routing::get,
    Router,
};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::info;

pub mod api;
pub mod state;

/// Start the mapit HTTP server on `127.0.0.1:<port>`.
/// Blocks until the server shuts down (e.g., via Ctrl+C).
pub async fn serve(db_path: &Path, port: u16) -> Result<()> {
    let store = mapit_core::graph::store::GraphStore::open(db_path)
        .with_context(|| format!("opening graph store at {}", db_path.display()))?;
    let store = Arc::new(Mutex::new(store));

    let (ws_tx, _) = broadcast::channel(256);

    let app_state = Arc::new(state::AppState {
        store,
        ws_tx: ws_tx.clone(),
        progress: Arc::new(Mutex::new(state::ProgressState::default())),
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
