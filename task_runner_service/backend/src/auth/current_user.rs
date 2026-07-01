// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
}

impl CurrentUser {
    pub fn public_user(&self) -> AuthUser {
        AuthUser {
            id: self.id.clone(),
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            role: self.role,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role == UserRole::Admin
    }

    pub fn is_agent(&self) -> bool {
        self.role == UserRole::Agent
    }

    pub fn effective_owner_user_id(&self) -> Option<&str> {
        self.owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn effective_owner_username(&self) -> Option<&str> {
        self.owner_username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn effective_owner_display_name(&self) -> Option<&str> {
        self.owner_display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn can_access_owned_resource(&self, owner_user_id: Option<&str>) -> bool {
        if self.is_admin() {
            return true;
        }
        let owner_user_id = owner_user_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match owner_user_id {
            Some(owner_user_id) => self.effective_owner_user_id() == Some(owner_user_id),
            None => false,
        }
    }
}

impl From<&UserRecord> for CurrentUser {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            role: value.role,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
        }
    }
}
