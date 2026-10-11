//! Indy_Reads server: Axum HTTP + WebSocket, static web UI.
//!
//! Pipeline per turn:
//!   utterance -> governor.select_provider -> provider.call -> governor.decide
//!     -> validate -> response -> turn.state -> receipt
//! Every step emits a hash-chained receipt. The receipt stream goes over WebSocket.

mod api;

use governor::AppState;
use indy_reads::governor;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!(
        "indy-reads starting at {}",
        indy_reads::time_util::now_rfc3339()
    );
    let state = Arc::new(AppState::new());
    let app = api::router(state);
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("indy-reads listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
