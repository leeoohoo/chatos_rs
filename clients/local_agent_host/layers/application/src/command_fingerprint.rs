// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::HostCommand;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

/// Hash the same JSON bytes used by legacy receipts without allocating another
/// complete copy of a potentially large command. v1 preserves serde_json's
/// existing representation; a future representation change needs a new version.
pub(super) fn fingerprint(command: &HostCommand) -> Result<String, serde_json::Error> {
    let mut writer = FingerprintWriter(Sha256::new());
    serde_json::to_writer(&mut writer, command)?;
    Ok(format!("sha256:v1:{:x}", writer.0.finalize()))
}

struct FingerprintWriter(Sha256);

impl Write for FingerprintWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::CreateRunCommand;
    use serde_json::json;

    fn command(message: &str) -> HostCommand {
        HostCommand::CreateRun(CreateRunCommand {
            run_id: "run-1".into(),
            owner_user_id: "alice".into(),
            owner_entity_type: "conversation".into(),
            owner_entity_id: "conversation-1".into(),
            profile_key: "main_chat".into(),
            model_config_ref: "model-1".into(),
            model_config_revision: "revision-1".into(),
            capability_policy_revision: "policy-1".into(),
            input: json!({"message": message, "nested": {"z": 2, "a": [true, null]}}),
            max_iterations: 4,
        })
    }

    #[test]
    fn streaming_fingerprint_matches_legacy_json_bytes() {
        for message in [
            "hello",
            "中文\n\"quoted\"\\path",
            &"x".repeat(2 * 1024 * 1024),
        ] {
            let command = command(message);
            let legacy = serde_json::to_string(&command).expect("legacy JSON");
            let digest = fingerprint(&command).expect("fingerprint");
            assert_eq!(
                digest,
                format!("sha256:v1:{:x}", Sha256::digest(legacy.as_bytes()))
            );
            assert_eq!(digest.len(), 74);
        }
    }

    #[test]
    fn fingerprint_distinguishes_content_and_owner() {
        let first = command("hello");
        assert_ne!(
            fingerprint(&first).unwrap(),
            fingerprint(&command("changed")).unwrap()
        );
        let mut other_owner = first.clone();
        if let HostCommand::CreateRun(command) = &mut other_owner {
            command.owner_user_id = "bob".into();
        }
        assert_ne!(
            fingerprint(&first).unwrap(),
            fingerprint(&other_owner).unwrap()
        );
    }
}
