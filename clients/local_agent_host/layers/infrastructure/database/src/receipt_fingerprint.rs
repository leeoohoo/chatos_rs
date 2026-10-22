// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sha2::{Digest, Sha256};

const PREFIX: &str = "sha256:v1:";

pub(super) fn compact_legacy(stored: &str) -> String {
    // Only the legacy full-JSON representation can be upgraded without
    // changing the meaning of opaque fingerprints used by trusted callers.
    if stored.starts_with('{') {
        digest(stored)
    } else {
        stored.to_owned()
    }
}

pub(super) fn matches(stored: &str, requested: &str) -> bool {
    if stored == requested {
        return true;
    }
    // Legacy application receipts contain the complete JSON object. Keep those
    // replayable after upgrading; do not delete their side-effect protection.
    if stored.starts_with('{') && requested.starts_with(PREFIX) {
        return digest(stored) == requested;
    }
    // Also support an older in-process caller against the upgraded adapter.
    requested.starts_with('{') && stored.starts_with(PREFIX) && digest(requested) == stored
}

fn digest(json: &str) -> String {
    format!("{PREFIX}{:x}", Sha256::digest(json.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_both_receipt_formats_without_weakening_content_checks() {
        let legacy = r#"{"type":"create_run","owner_user_id":"alice"}"#;
        let compact = digest(legacy);
        assert!(matches(legacy, legacy));
        assert!(matches(&compact, &compact));
        assert!(matches(legacy, &compact));
        assert!(matches(&compact, legacy));
        let changed = r#"{"type":"create_run","owner_user_id":"bob"}"#;
        assert!(!matches(legacy, &digest(changed)));
        assert!(!matches(&compact, changed));
        assert!(!matches(&compact, &digest(changed)));
        assert!(!matches("opaque", &digest("opaque")));
    }
}
