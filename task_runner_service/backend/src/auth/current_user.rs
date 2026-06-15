use super::*;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
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
}

impl From<&UserRecord> for CurrentUser {
    fn from(value: &UserRecord) -> Self {
        Self {
            id: value.id.clone(),
            username: value.username.clone(),
            display_name: value.display_name.clone(),
            role: value.role,
        }
    }
}
