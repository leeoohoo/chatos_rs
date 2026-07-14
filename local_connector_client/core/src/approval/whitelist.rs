// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use uuid::Uuid;

use crate::local_now_rfc3339;

use super::fingerprint::{command_fingerprint, normalized_command};
use super::types::{ApprovalProjectKey, ApprovalSource, CommandWhitelistEntry, WhitelistCwdScope};

pub(crate) fn find_matching_whitelist<'a>(
    entries: &'a [CommandWhitelistEntry],
    project_key: &ApprovalProjectKey,
    command: &str,
    args: &[String],
    cwd: &str,
) -> Option<&'a CommandWhitelistEntry> {
    let project_fingerprint = command_fingerprint(command, args, ".");
    let cwd_fingerprint = command_fingerprint(command, args, cwd);
    entries.iter().find(|entry| {
        entry.enabled
            && whitelist_entry_can_authorize(entry)
            && &entry.project_key == project_key
            && match entry.cwd_scope {
                WhitelistCwdScope::Project => entry.command_fingerprint == project_fingerprint,
                WhitelistCwdScope::Cwd => entry.command_fingerprint == cwd_fingerprint,
            }
    })
}

pub(crate) fn build_whitelist_entry(
    project_key: ApprovalProjectKey,
    command: &str,
    args: &[String],
    cwd: &str,
    cwd_scope: WhitelistCwdScope,
    created_by: ApprovalSource,
) -> CommandWhitelistEntry {
    let fingerprint_cwd = match cwd_scope {
        WhitelistCwdScope::Project => ".",
        WhitelistCwdScope::Cwd => cwd,
    };
    CommandWhitelistEntry {
        id: format!("allow-{}", Uuid::new_v4()),
        project_key,
        command_fingerprint: command_fingerprint(command, args, fingerprint_cwd),
        command_display: normalized_command(command, args),
        normalized_command: normalized_command(command, args),
        cwd_scope,
        created_by,
        created_at: local_now_rfc3339(),
        enabled: true,
    }
}

fn whitelist_entry_can_authorize(entry: &CommandWhitelistEntry) -> bool {
    entry.created_by == ApprovalSource::User
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_key() -> ApprovalProjectKey {
        ApprovalProjectKey {
            owner_user_id: "owner".to_string(),
            device_id: "device".to_string(),
            workspace_id: "workspace".to_string(),
            project_id: Some("project".to_string()),
            project_root_relative_path: ".".to_string(),
            project_anchor_relative_path: None,
        }
    }

    #[test]
    fn whitelist_matching_ignores_ai_created_entries() {
        let key = project_key();
        let command = "npm";
        let args = vec!["test".to_string()];
        let cwd = ".";
        let ai_entry = build_whitelist_entry(
            key.clone(),
            command,
            args.as_slice(),
            cwd,
            WhitelistCwdScope::Project,
            ApprovalSource::Ai,
        );
        let user_entry = build_whitelist_entry(
            key.clone(),
            command,
            args.as_slice(),
            cwd,
            WhitelistCwdScope::Project,
            ApprovalSource::User,
        );

        assert!(
            find_matching_whitelist(&[ai_entry], &key, command, args.as_slice(), cwd).is_none()
        );
        assert_eq!(
            find_matching_whitelist(&[user_entry.clone()], &key, command, args.as_slice(), cwd)
                .map(|entry| entry.id.as_str()),
            Some(user_entry.id.as_str())
        );
    }
}
