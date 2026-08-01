// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn authorization_url_uses_pkce_state_resource_and_exact_scopes() {
    let url = build_authorization_url(
        "https://auth.example.com/authorize?ignored=true",
        "client-1",
        "https://plugins.example.com/api/plugins/cloud-oauth/callback",
        "state-1",
        "challenge-1",
        "https://mcp.example.com/mcp",
        &["files:read".to_string(), "files:write".to_string()],
    )
    .unwrap();
    let values = url.query_pairs().collect::<BTreeMap<_, _>>();
    assert_eq!(
        values.get("response_type").map(|value| value.as_ref()),
        Some("code")
    );
    assert_eq!(
        values
            .get("code_challenge_method")
            .map(|value| value.as_ref()),
        Some("S256")
    );
    assert_eq!(
        values.get("resource").map(|value| value.as_ref()),
        Some("https://mcp.example.com/mcp")
    );
    assert_eq!(
        values.get("scope").map(|value| value.as_ref()),
        Some("files:read files:write")
    );
    assert!(!values.contains_key("ignored"));
}

#[test]
fn metadata_locations_follow_resource_and_issuer_paths() {
    let resource = Url::parse("https://mcp.example.com/v1/mcp").unwrap();
    let resource_urls = protected_resource_metadata_urls(&resource).unwrap();
    assert_eq!(
        resource_urls[0].as_str(),
        "https://mcp.example.com/.well-known/oauth-protected-resource/v1/mcp"
    );
    let issuer = Url::parse("https://auth.example.com/tenant").unwrap();
    let issuer_urls = authorization_server_metadata_urls(&issuer).unwrap();
    assert_eq!(
        issuer_urls[0].as_str(),
        "https://auth.example.com/.well-known/oauth-authorization-server/tenant"
    );
    assert_eq!(
        issuer_urls[1].as_str(),
        "https://auth.example.com/tenant/.well-known/openid-configuration"
    );
}

#[test]
fn token_auth_method_requires_matching_secret_shape() {
    assert_eq!(
        normalize_token_endpoint_auth_method(None, false).unwrap(),
        "none"
    );
    assert_eq!(
        normalize_token_endpoint_auth_method(None, true).unwrap(),
        "client_secret_basic"
    );
    assert!(normalize_token_endpoint_auth_method(Some("none"), true).is_err());
    assert!(normalize_token_endpoint_auth_method(Some("client_secret_post"), false).is_err());
}

#[test]
fn returned_oauth_scopes_must_match_the_signed_request_exactly() {
    let requested = vec!["files:read".to_string(), "files:write".to_string()];
    assert_eq!(
        normalized_token_scopes(Some("files:write files:read"), &requested).unwrap(),
        requested
    );
    assert!(normalized_token_scopes(Some("files:read"), &requested).is_err());
    assert!(normalized_token_scopes(Some("files:read files:write admin"), &requested).is_err());
}

#[test]
fn oauth_refresh_window_is_fail_closed_for_invalid_expiry() {
    let mut connection = PluginCloudOAuthConnectionRecord {
        id: "oauth-1".to_string(),
        owner_user_id: "owner-1".to_string(),
        plugin_id: "plugin-1".to_string(),
        release_id: "release-1".to_string(),
        component_key: "mcp-1".to_string(),
        provider: "figma".to_string(),
        resource: "https://mcp.example.com/mcp".to_string(),
        scopes: vec!["files:read".to_string()],
        connected: true,
        needs_auth: false,
        refreshable: true,
        expires_at: Some((Utc::now() + ChronoDuration::seconds(30)).to_rfc3339()),
        account_display: None,
        revision: "revision-1".to_string(),
        updated_at: now_rfc3339(),
    };
    assert!(oauth_access_token_needs_refresh(&connection, Utc::now().timestamp() + 90).unwrap());
    connection.expires_at = Some("invalid".to_string());
    assert!(oauth_access_token_needs_refresh(&connection, Utc::now().timestamp() + 90).is_err());
}

#[test]
fn callback_html_never_contains_oauth_tokens() {
    let response = oauth_callback_response(
        "https://plugins.example.com",
        Err("OAuth authorization failed".to_string()),
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );
    assert_eq!(
        response.headers().get(header::REFERRER_POLICY).unwrap(),
        "no-referrer"
    );
}

#[test]
fn private_and_special_addresses_are_rejected() {
    for value in [
        "127.0.0.1",
        "10.0.0.1",
        "169.254.1.1",
        "192.168.1.1",
        "100.64.0.1",
        "198.18.0.1",
        "::1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
    ] {
        assert!(!is_public_ip(value.parse().unwrap()), "{value}");
    }
    assert!(is_public_ip("8.8.8.8".parse().unwrap()));
    assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
}
