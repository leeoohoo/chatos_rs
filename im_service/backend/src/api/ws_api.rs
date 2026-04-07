use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use super::shared::{require_auth, require_auth_from_access_token};
use super::SharedState;

#[derive(Debug, Default, Deserialize)]
pub(super) struct ImWsQuery {
    access_token: Option<String>,
}

pub(super) async fn im_events_ws(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ImWsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let auth = if let Some(access_token) = query
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        match require_auth_from_access_token(&state, access_token) {
            Ok(value) => value,
            Err(err) => return err.into_response(),
        }
    } else {
        match require_auth(&state, &headers) {
            Ok(value) => value,
            Err(err) => return err.into_response(),
        }
    };

    ws.on_upgrade(move |socket| handle_im_socket(state, auth.user_id, socket))
}

async fn handle_im_socket(state: SharedState, user_id: String, socket: WebSocket) {
    let mut receiver = state.event_hub.subscribe(user_id.as_str());
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();
    let (mut sender, mut receiver_ws) = socket.split();

    let _ = outbound_tx.send(
        json!({
            "type": "im.connected",
            "user_id": user_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
        .to_string(),
    );

    let writer_task = tokio::spawn(async move {
        while let Some(payload) = outbound_rx.recv().await {
            if sender.send(Message::Text(payload)).await.is_err() {
                break;
            }
        }
    });

    let relay_tx = outbound_tx.clone();
    let relay_task = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(payload) => {
                    if relay_tx.send(payload.to_string()).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(msg) = receiver_ws.next().await {
        match msg {
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Text(_)) | Ok(Message::Binary(_)) => {}
            Err(_) => break,
        }
    }

    relay_task.abort();
    writer_task.abort();
    let _ = relay_task.await;
    let _ = writer_task.await;
}
