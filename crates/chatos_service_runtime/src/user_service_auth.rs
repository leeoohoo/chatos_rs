// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct UserServiceLoginRequest<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceAuthUser {
    pub id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub principal_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceLoginResponse {
    pub token: String,
    pub user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceVerifiedPrincipal {
    pub principal_type: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub agent_account_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceVerifyResponse {
    pub principal: UserServiceVerifiedPrincipal,
}
