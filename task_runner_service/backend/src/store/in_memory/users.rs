use super::*;

impl InMemoryStore {
    pub(in crate::store) fn count_users(&self) -> i64 {
        self.inner.read().users.len() as i64
    }

    pub(in crate::store) fn list_users(&self) -> Vec<UserRecord> {
        let data = self.inner.read();
        let mut items = data.users.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then(left.username.cmp(&right.username))
        });
        items
    }

    pub(in crate::store) fn get_user(&self, id: &str) -> Option<UserRecord> {
        self.inner.read().users.get(id).cloned()
    }

    pub(in crate::store) fn get_user_by_username(&self, username: &str) -> Option<UserRecord> {
        self.inner
            .read()
            .users
            .values()
            .find(|user| user.username.eq_ignore_ascii_case(username))
            .cloned()
    }

    pub(in crate::store) fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        let mut data = self.inner.write();
        if data
            .users
            .values()
            .any(|existing| existing.id != user.id && existing.username == user.username)
        {
            return Err(format!("用户名已存在: {}", user.username));
        }
        data.users.insert(user.id.clone(), user.clone());
        Ok(user)
    }

    pub(in crate::store) fn delete_user(&self, id: &str) -> bool {
        self.inner.write().users.remove(id).is_some()
    }
}
