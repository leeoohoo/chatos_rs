use std::collections::HashMap;
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct AbortEntry {
    token: CancellationToken,
    aborted: bool,
    turn_id: Option<String>,
}

static ABORT_REGISTRY: Lazy<Arc<Mutex<HashMap<String, AbortEntry>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

pub fn set_controller(session_id: &str, turn_id: Option<&str>, token: CancellationToken) {
    if session_id.is_empty() {
        return;
    }
    let normalized_turn_id = turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut map = ABORT_REGISTRY.lock();
    let entry = map.entry(session_id.to_string()).or_insert(AbortEntry {
        token: token.clone(),
        aborted: false,
        turn_id: normalized_turn_id.clone(),
    });
    entry.token = token;
    entry.turn_id = normalized_turn_id;
    if entry.aborted {
        entry.token.cancel();
    }
}

pub fn get_controller(session_id: &str) -> Option<CancellationToken> {
    let map = ABORT_REGISTRY.lock();
    map.get(session_id).map(|e| e.token.clone())
}

pub fn abort(session_id: &str) -> bool {
    abort_turn(session_id, None)
}

pub fn abort_turn(session_id: &str, turn_id: Option<&str>) -> bool {
    if session_id.is_empty() {
        return false;
    }
    let normalized_turn_id = turn_id.map(str::trim).filter(|value| !value.is_empty());
    let mut map = ABORT_REGISTRY.lock();
    if let Some(entry) = map.get_mut(session_id) {
        if let Some(target_turn_id) = normalized_turn_id {
            let Some(active_turn_id) = entry.turn_id.as_deref() else {
                return false;
            };
            if active_turn_id != target_turn_id {
                return false;
            }
        }
        entry.aborted = true;
        entry.token.cancel();
        return true;
    }
    // Mirror Node behavior: mark aborted even if no controller exists yet.
    if normalized_turn_id.is_some() {
        return false;
    }
    map.insert(
        session_id.to_string(),
        AbortEntry {
            token: CancellationToken::new(),
            aborted: true,
            turn_id: None,
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
    reset_turn(session_id, None);
}

pub fn reset_turn(session_id: &str, turn_id: Option<&str>) {
    if session_id.is_empty() {
        return;
    }
    let normalized_turn_id = turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut map = ABORT_REGISTRY.lock();
    let entry = map.entry(session_id.to_string()).or_insert(AbortEntry {
        token: CancellationToken::new(),
        aborted: false,
        turn_id: normalized_turn_id.clone(),
    });
    entry.aborted = false;
    entry.turn_id = normalized_turn_id;
}

pub fn clear(session_id: &str) {
    if session_id.is_empty() {
        return;
    }
    let mut map = ABORT_REGISTRY.lock();
    map.remove(session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_controller_cancels_new_token_when_session_already_aborted() {
        let session_id = "abort_registry_set_controller_cancels_token";
        clear(session_id);

        // Simulate stop arriving before controller registration.
        assert!(abort(session_id));
        assert!(is_aborted(session_id));

        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        set_controller(session_id, None, token.clone());

        assert!(token.is_cancelled());
        clear(session_id);
    }

    #[test]
    fn abort_turn_ignores_mismatched_active_turn() {
        let session_id = "abort_registry_abort_turn_mismatch";
        clear(session_id);

        let token = CancellationToken::new();
        set_controller(session_id, Some("turn_new"), token.clone());

        assert!(!abort_turn(session_id, Some("turn_old")));
        assert!(!token.is_cancelled());
        assert!(!is_aborted(session_id));

        clear(session_id);
    }

    #[test]
    fn abort_turn_cancels_matching_active_turn() {
        let session_id = "abort_registry_abort_turn_match";
        clear(session_id);

        let token = CancellationToken::new();
        set_controller(session_id, Some("turn_same"), token.clone());

        assert!(abort_turn(session_id, Some("turn_same")));
        assert!(token.is_cancelled());
        assert!(is_aborted(session_id));

        clear(session_id);
    }

    #[test]
    fn reset_turn_protects_next_turn_before_controller_registration() {
        let session_id = "abort_registry_reset_turn_protects_next";
        clear(session_id);

        let old_token = CancellationToken::new();
        set_controller(session_id, Some("turn_old"), old_token.clone());
        reset_turn(session_id, Some("turn_new"));

        assert!(!abort_turn(session_id, Some("turn_old")));

        let new_token = CancellationToken::new();
        set_controller(session_id, Some("turn_new"), new_token.clone());

        assert!(!new_token.is_cancelled());
        assert!(!is_aborted(session_id));

        clear(session_id);
    }

    #[test]
    fn reset_turn_allows_current_turn_stop_before_controller_registration() {
        let session_id = "abort_registry_reset_turn_allows_current_stop";
        clear(session_id);

        reset_turn(session_id, Some("turn_new"));
        assert!(abort_turn(session_id, Some("turn_new")));

        let token = CancellationToken::new();
        set_controller(session_id, Some("turn_new"), token.clone());

        assert!(token.is_cancelled());

        clear(session_id);
    }
}
