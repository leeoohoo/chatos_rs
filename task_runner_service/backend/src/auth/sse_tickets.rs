// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use uuid::Uuid;

const SSE_TICKET_TTL_SECONDS: i64 = 60;

#[derive(Debug, Clone)]
pub struct SseTicketRecord {
    pub access_token: String,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SseTicketStore {
    tickets: Arc<RwLock<BTreeMap<String, SseTicketRecord>>>,
}

impl SseTicketStore {
    pub fn issue(&self, access_token: &str) -> IssuedSseTicket {
        let now = Utc::now().timestamp();
        let ticket = Uuid::new_v4().to_string();
        let expires_at_unix = now + SSE_TICKET_TTL_SECONDS;
        self.tickets.write().insert(
            ticket.clone(),
            SseTicketRecord {
                access_token: access_token.trim().to_string(),
                expires_at_unix,
            },
        );
        IssuedSseTicket {
            ticket,
            expires_in: SSE_TICKET_TTL_SECONDS,
            expires_at_unix,
        }
    }

    pub fn consume(&self, ticket: &str) -> Option<SseTicketRecord> {
        let ticket = ticket.trim();
        if ticket.is_empty() {
            return None;
        }

        let mut tickets = self.tickets.write();
        let record = tickets.remove(ticket)?;
        if record.expires_at_unix <= Utc::now().timestamp() {
            return None;
        }
        Some(record)
    }
}

#[derive(Debug, Clone)]
pub struct IssuedSseTicket {
    pub ticket: String,
    pub expires_in: i64,
    pub expires_at_unix: i64,
}

#[cfg(test)]
mod tests {
    use super::SseTicketStore;

    #[test]
    fn issued_ticket_can_only_be_consumed_once() {
        let store = SseTicketStore::default();
        let issued = store.issue("access-token");

        let first = store
            .consume(issued.ticket.as_str())
            .expect("first consume");
        assert_eq!(first.access_token, "access-token");
        assert!(store.consume(issued.ticket.as_str()).is_none());
    }
}
