use super::support::{hash_password, normalize_display_name, normalize_username, verify_password};
use super::*;

mod login;
mod sessions;
mod users;

#[derive(Debug, Clone)]
struct SessionRecord {
    user: CurrentUser,
    expires_at_unix: Option<i64>,
}

#[derive(Clone)]
pub struct AuthService {
    store: AppStore,
    sessions: Arc<RwLock<BTreeMap<String, SessionRecord>>>,
}

impl AuthService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            store,
            sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}
