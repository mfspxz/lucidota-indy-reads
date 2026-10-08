//! Indy_Reads server: Axum HTTP + WebSocket, static web UI.
//!
//! Pipeline per turn:
//!   utterance -> governor.select_provider -> provider.call -> governor.decide
//!     -> validate -> response -> turn.state -> receipt
//! Every step emits a hash-chained receipt. The receipt stream goes over WebSocket.

mod ai;
mod api;

use indy_reads::{domain, governor, receipts};
use governor::AppState;
use std::sync::Arc;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
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
