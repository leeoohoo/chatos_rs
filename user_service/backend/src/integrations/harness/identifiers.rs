// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::UserRecord;
use crate::state::AppState;

const HARNESS_MAX_IDENTIFIER_LEN: usize = 100;
const HARNESS_MAX_EMAIL_LEN: usize = 250;

pub(super) fn harness_uid_for_user(user: &UserRecord) -> String {
    let username = user.username.trim().to_ascii_lowercase();
    if is_harness_identifier(username.as_str()) && !username.eq_ignore_ascii_case("anonymous") {
        return username;
    }
    format!("chatos-{}", compact_user_id(user.id.as_str()))
}

pub(super) fn harness_project_pat_identifier(state: &AppState, user: &UserRecord) -> String {
    let prefix = sanitize_harness_identifier_part(
        state.config.harness_project_pat_prefix.as_str(),
        "chatos-project-import",
    );
    let user_part = sanitize_harness_identifier_part(user.username.as_str(), "user");
    let suffix = compact_user_id(uuid::Uuid::new_v4().to_string().as_str());
    truncate_harness_identifier(format!("{prefix}-{user_part}-{suffix}").as_str())
}

pub(super) fn harness_repo_identifier(project_name: &str, project_id: &str) -> String {
    let name = sanitize_harness_identifier_part(project_name, "project");
    let suffix = compact_user_id(project_id);
    truncate_harness_identifier(format!("{name}-{suffix}").as_str())
}

pub(super) fn harness_email_for_user(
    user: &UserRecord,
    uid: &str,
    synthetic_email_domain: &str,
) -> String {
    let username = user.username.trim();
    if username.contains('@') && !username.is_empty() && username.len() <= HARNESS_MAX_EMAIL_LEN {
        return username.to_ascii_lowercase();
    }

    let domain = synthetic_email_domain
        .trim()
        .trim_start_matches('@')
        .trim_matches('.')
        .to_ascii_lowercase();
    let domain = if domain.is_empty() {
        "chatos.local".to_string()
    } else {
        domain
    };
    let email = format!("{uid}@{domain}");
    if email.len() <= HARNESS_MAX_EMAIL_LEN {
        email
    } else {
        format!("{}@chatos.local", compact_user_id(user.id.as_str()))
    }
}

pub(super) fn harness_space_identifier_for_user(
    user: &UserRecord,
    uid: &str,
    harness_space_prefix: &str,
) -> String {
    let prefix = harness_space_prefix.trim();
    let prefix = if prefix.is_empty() { "u-" } else { prefix };
    let candidate = format!("{prefix}{uid}");
    if is_valid_root_space_identifier(candidate.as_str()) {
        return candidate;
    }

    let fallback = format!("u-{}", compact_user_id(user.id.as_str()));
    if is_valid_root_space_identifier(fallback.as_str()) {
        fallback
    } else {
        "u-chatos-user".to_string()
    }
}

fn compact_user_id(user_id: &str) -> String {
    let compact: String = user_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(12)
        .collect();
    if compact.is_empty() {
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .take(12)
            .collect()
    } else {
        compact.to_ascii_lowercase()
    }
}

fn sanitize_harness_identifier_part(value: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_ascii_lowercase().chars() {
        let next = if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.') {
            ch
        } else if ch == '-' || ch.is_whitespace() {
            '-'
        } else {
            '-'
        };
        if next == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        out.push(next);
    }
    let out = out.trim_matches('-').trim_matches('.').to_string();
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

fn truncate_harness_identifier(value: &str) -> String {
    let trimmed = value.trim().trim_matches('-').trim_matches('.');
    if trimmed.len() <= HARNESS_MAX_IDENTIFIER_LEN {
        return trimmed.to_string();
    }
    trimmed
        .chars()
        .take(HARNESS_MAX_IDENTIFIER_LEN)
        .collect::<String>()
        .trim_matches('-')
        .trim_matches('.')
        .to_string()
}

pub(super) fn truncate_error(error: &str) -> String {
    const MAX_ERROR_LEN: usize = 1000;
    let trimmed = error.trim();
    if trimmed.len() <= MAX_ERROR_LEN {
        trimmed.to_string()
    } else {
        trimmed.chars().take(MAX_ERROR_LEN).collect()
    }
}

fn is_harness_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= HARNESS_MAX_IDENTIFIER_LEN
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn is_valid_root_space_identifier(value: &str) -> bool {
    if !is_harness_identifier(value) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if lower == "api" || lower == "git" || lower.ends_with(".git") {
        return false;
    }
    !value.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{
        harness_email_for_user, harness_space_identifier_for_user, harness_uid_for_user,
        is_valid_root_space_identifier,
    };
    use crate::models::{UserRecord, USER_ROLE_USER};

    fn test_user(username: &str) -> UserRecord {
        UserRecord {
            id: "12345678-90ab-cdef-1234-567890abcdef".to_string(),
            username: username.to_string(),
            display_name: username.to_string(),
            password_hash: "hash".to_string(),
            role: USER_ROLE_USER.to_string(),
            enabled: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            last_login_at: None,
        }
    }

    #[test]
    fn harness_uid_reuses_valid_username() {
        let user = test_user("leeoohoo");
        assert_eq!(harness_uid_for_user(&user), "leeoohoo");
    }

    #[test]
    fn harness_uid_falls_back_for_email_username() {
        let user = test_user("alice@example.com");
        assert_eq!(harness_uid_for_user(&user), "chatos-1234567890ab");
        assert_eq!(
            harness_email_for_user(&user, "chatos-1234567890ab", "chatos.local"),
            "alice@example.com"
        );
    }

    #[test]
    fn harness_email_uses_synthetic_domain_for_plain_username() {
        let user = test_user("leeoohoo");
        assert_eq!(
            harness_email_for_user(&user, "leeoohoo", "@example.internal."),
            "leeoohoo@example.internal"
        );
    }

    #[test]
    fn harness_space_identifier_uses_prefix_and_fallback() {
        let user = test_user("leeoohoo");
        assert_eq!(
            harness_space_identifier_for_user(&user, "leeoohoo", "u-"),
            "u-leeoohoo"
        );
        assert_eq!(
            harness_space_identifier_for_user(&user, "leeoohoo", "bad@"),
            "u-1234567890ab"
        );
    }

    #[test]
    fn root_space_identifier_rejects_harness_reserved_values() {
        assert!(!is_valid_root_space_identifier("12345"));
        assert!(!is_valid_root_space_identifier("api"));
        assert!(!is_valid_root_space_identifier("git"));
        assert!(!is_valid_root_space_identifier("project.git"));
        assert!(is_valid_root_space_identifier("u-leeoohoo"));
    }
}
