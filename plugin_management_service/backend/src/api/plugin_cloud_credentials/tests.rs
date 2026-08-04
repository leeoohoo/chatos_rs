// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn template_parser_never_accepts_multiple_secret_placeholders() {
    let parsed = parse_template("Bearer ${credential:access_token}").unwrap();
    assert_eq!(parsed.secret_name.as_deref(), Some("access_token"));
    assert_eq!(parsed.prefix, "Bearer ");
    assert!(parse_template("${credential:first}-${credential:second}").is_err());
}

#[test]
fn oauth_permissions_are_provider_and_scope_exact() {
    assert!(require_oauth_permissions(
        "figma",
        &["files:read".to_string()],
        &["oauth.scope:figma:files:read".to_string()],
    )
    .is_ok());
    assert!(require_oauth_permissions(
        "figma",
        &["files:write".to_string()],
        &["oauth.scope:figma:files:read".to_string()],
    )
    .is_err());
}
