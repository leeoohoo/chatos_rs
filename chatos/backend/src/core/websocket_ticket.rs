// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::Serialize;
use uuid::Uuid;

use crate::core::auth::{AuthHeaderError, AuthUser};

const DEFAULT_WS_TICKET_TTL_SECONDS: i64 = 45;

static WS_TICKET_STORE: Lazy<DashMap<String, WebSocketTicketRecord>> = Lazy::new(DashMap::new);

#[derive(Debug, Clone)]
pub struct WebSocketTicketRecord {
    pub access_token: String,
    pub auth_user: AuthUser,
    pub expires_at_epoch_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebSocketTicketResponse {
    pub ticket: String,
    pub expires_in: i64,
    pub expires_at: String,
}

pub fn issue_websocket_ticket(
    access_token: &str,
    auth_user: &AuthUser,
) -> Result<WebSocketTicketResponse, AuthHeaderError> {
    let normalized_access_token = access_token.trim();
    if normalized_access_token.is_empty() {
        return Err(AuthHeaderError::InvalidOrExpiredToken);
    }

    prune_expired_tickets();

    let expires_at_epoch_seconds = chrono::Utc::now().timestamp() + DEFAULT_WS_TICKET_TTL_SECONDS;
    let ticket = format!("ws_{}", Uuid::new_v4().simple());
    let record = WebSocketTicketRecord {
        access_token: normalized_access_token.to_string(),
        auth_user: auth_user.clone(),
        expires_at_epoch_seconds,
    };
    WS_TICKET_STORE.insert(ticket.clone(), record);

    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(expires_at_epoch_seconds, 0)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(crate::core::time::now_rfc3339);

    Ok(WebSocketTicketResponse {
        ticket,
        expires_in: DEFAULT_WS_TICKET_TTL_SECONDS,
        expires_at,
    })
}

pub fn consume_websocket_ticket(ticket: &str) -> Result<WebSocketTicketRecord, AuthHeaderError> {
    let normalized_ticket = ticket.trim();
    if normalized_ticket.is_empty() {
        return Err(AuthHeaderError::InvalidOrExpiredToken);
    }

    prune_expired_tickets();

    let Some((_, record)) = WS_TICKET_STORE.remove(normalized_ticket) else {
        return Err(AuthHeaderError::InvalidOrExpiredToken);
    };

    if record.expires_at_epoch_seconds <= chrono::Utc::now().timestamp() {
        return Err(AuthHeaderError::InvalidOrExpiredToken);
    }

    Ok(record)
}

fn prune_expired_tickets() {
    let now = chrono::Utc::now().timestamp();
    WS_TICKET_STORE.retain(|_, record| record.expires_at_epoch_seconds > now);
}

#[cfg(test)]
mod tests {
    use super::{consume_websocket_ticket, issue_websocket_ticket};
    use crate::core::auth::AuthUser;

    fn build_auth_user() -> AuthUser {
        AuthUser {
            user_id: "user_1".to_string(),
            role: "user".to_string(),
        }
    }

    #[test]
    fn websocket_ticket_is_single_use() {
        let response =
            issue_websocket_ticket("access_token_1", &build_auth_user()).expect("issue ticket");

        let consumed = consume_websocket_ticket(response.ticket.as_str()).expect("consume ticket");
        assert_eq!(consumed.access_token, "access_token_1");
        assert_eq!(consumed.auth_user.user_id, "user_1");

        assert!(consume_websocket_ticket(response.ticket.as_str()).is_err());
    }

    #[test]
    fn websocket_ticket_rejects_blank_access_token() {
        assert!(issue_websocket_ticket("   ", &build_auth_user()).is_err());
    }
}
