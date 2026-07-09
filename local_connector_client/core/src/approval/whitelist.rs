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
