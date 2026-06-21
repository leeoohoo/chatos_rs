use super::support::{hash_password, normalize_display_name, normalize_username, verify_password};
use super::*;

mod external_tokens;
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
    config: AppConfig,
    store: AppStore,
    sessions: Arc<RwLock<BTreeMap<String, SessionRecord>>>,
}

impl AuthService {
    pub(crate) fn new(config: AppConfig, store: AppStore) -> Self {
        Self {
            config,
            store,
            sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}
