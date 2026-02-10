use std::collections::HashMap;
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct AbortEntry {
    token: CancellationToken,
    aborted: bool,
}

static ABORT_REGISTRY: Lazy<Arc<Mutex<HashMap<String, AbortEntry>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

pub fn set_controller(session_id: &str, token: CancellationToken) {
    if session_id.is_empty() {
        return;
    }
    let mut map = ABORT_REGISTRY.lock();
    let entry = map.entry(session_id.to_string()).or_insert(AbortEntry {
        token: token.clone(),
        aborted: false,
    });
    entry.token = token;
}

pub fn get_controller(session_id: &str) -> Option<CancellationToken> {
    let map = ABORT_REGISTRY.lock();
    map.get(session_id).map(|e| e.token.clone())
}

pub fn abort(session_id: &str) -> bool {
    if session_id.is_empty() {
        return false;
    }
    let mut map = ABORT_REGISTRY.lock();
    if let Some(entry) = map.get_mut(session_id) {
        entry.aborted = true;
        entry.token.cancel();
        return true;
    }
    // Mirror Node behavior: mark aborted even if no controller exists yet.
    map.insert(
        session_id.to_string(),
        AbortEntry {
            token: CancellationToken::new(),
            aborted: true,
        },
    );
    true
}

pub fn is_aborted(session_id: &str) -> bool {
    if session_id.is_empty() {
        return false;
    }
    let map = ABORT_REGISTRY.lock();
    map.get(session_id).map(|e| e.aborted).unwrap_or(false)
}

pub fn reset(session_id: &str) {
    if session_id.is_empty() {
        return;
    }
    let mut map = ABORT_REGISTRY.lock();
    let entry = map.entry(session_id.to_string()).or_insert(AbortEntry {
        token: CancellationToken::new(),
        aborted: false,
    });
    entry.aborted = false;
}

pub fn clear(session_id: &str) {
    if session_id.is_empty() {
        return;
    }
    let mut map = ABORT_REGISTRY.lock();
    map.remove(session_id);
}
