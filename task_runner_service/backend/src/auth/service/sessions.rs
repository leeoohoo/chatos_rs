use super::*;

impl AuthService {
    pub fn current_user_for_token(&self, token: &str) -> Option<CurrentUser> {
        let token = token.trim();
        let now = Utc::now().timestamp();
        {
            let sessions = self.sessions.read();
            if let Some(record) = sessions.get(token) {
                if record
                    .expires_at_unix
                    .is_none_or(|expires_at| expires_at > now)
                {
                    return Some(record.user.clone());
                }
            }
        }
        self.sessions.write().remove(token);
        self.current_user_from_user_service_token(token)
    }

    pub fn logout(&self, token: &str) {
        self.sessions.write().remove(token.trim());
    }

    pub(super) fn create_session(&self, user: CurrentUser, ttl_seconds: Option<i64>) -> String {
        let token = Uuid::new_v4().to_string();
        let expires_at_unix = ttl_seconds.map(|ttl| Utc::now().timestamp() + ttl.max(1));
        self.sessions.write().insert(
            token.clone(),
            SessionRecord {
                user,
                expires_at_unix,
            },
        );
        token
    }

    pub(super) fn sync_user_sessions(&self, user: &UserRecord) {
        let mut sessions = self.sessions.write();
        if !user.enabled {
            sessions.retain(|_, current| current.user.id != user.id);
            return;
        }
        for record in sessions.values_mut() {
            if record.user.id == user.id {
                record.user.username = user.username.clone();
                record.user.display_name = user.display_name.clone();
                record.user.role = user.role;
            }
        }
    }

    pub(super) fn remove_user_sessions(&self, user_id: &str) {
        self.sessions
            .write()
            .retain(|_, current| current.user.id != user_id);
    }
}
