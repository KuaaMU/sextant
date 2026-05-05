//! Axum WebSocket server for Sextant event streaming.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;

use super::event_stream::StreamEvent;

/// Create the WebSocket router with the event broadcaster.
pub fn router(tx: broadcast::Sender<StreamEvent>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(tx)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(tx): State<broadcast::Sender<StreamEvent>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, tx))
}

async fn handle_socket(socket: WebSocket, tx: broadcast::Sender<StreamEvent>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = tx.subscribe();

    tracing::info!("WebSocket client connected");

    // Forward events from broadcast channel to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages (ping/pong, close)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                _ => {} // Ignore other messages
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    tracing::info!("WebSocket client disconnected");
}
