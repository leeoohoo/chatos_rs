// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::Utc;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{
    now_rfc3339, AgentTokenResponse, AuthUser, CreateUserRequest, LoginResponse, UpdateUserRequest,
    UserRecord, UserRole, UserSummaryRecord,
};
use crate::store::AppStore;

const AGENT_TOKEN_TTL_SECONDS: i64 = 3600;

mod access_token_scope;
mod current_user;
mod service;
mod sse_tickets;
mod support;

pub use access_token_scope::{
    get_current_access_token, spawn_with_current_access_token, with_access_token_scope,
};
pub use current_user::CurrentUser;
pub use service::AuthService;
pub use sse_tickets::{IssuedSseTicket, SseTicketStore};
