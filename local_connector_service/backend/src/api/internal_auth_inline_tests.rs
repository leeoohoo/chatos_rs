#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;

    const CHATOS_SECRET: &str = "a-long-chatos-local-connector-secret";

    #[test]
    fn system_stats_token_is_owner_scope_caller_and_path_bound() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(CHATOS_CALLER.to_string(), CHATOS_SECRET.to_string());
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            CHATOS_SECRET,
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            SYSTEM_STATS_READ_SCOPE,
            60,
            "user-1",
        )
        .expect("issue system stats token");
        let headers = signed_headers(CHATOS_CALLER, token.as_str(), "user-1");
        let (user, identity) = internal_service_auth_from_request(
            &config,
            &headers,
            &Method::GET,
            "/api/local-connectors/system/stats",
        )
        .expect("valid internal request")
        .expect("service identity");
        assert_eq!(user.effective_owner_user_id(), "user-1");
        assert_eq!(identity.scope, SYSTEM_STATS_READ_SCOPE);
        assert_eq!(identity.caller_service, CHATOS_CALLER);
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/system/stats",
        )
        .is_err());
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::GET,
            "/api/local-connectors/devices",
        )
        .is_err());
    }

    #[test]
    fn mismatched_owner_header_is_rejected() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(CHATOS_CALLER.to_string(), CHATOS_SECRET.to_string());
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            CHATOS_SECRET,
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            SYSTEM_STATS_READ_SCOPE,
            60,
            "user-1",
        )
        .expect("issue owner-bound token");
        let headers = signed_headers(CHATOS_CALLER, token.as_str(), "user-2");
        let error = internal_service_auth_from_request(
            &config,
            &headers,
            &Method::GET,
            "/api/local-connectors/system/stats",
        )
        .expect_err("mismatched owner header must be rejected");
        assert_eq!(
            error.message(),
            "Local Connector owner user id header does not match the signed token"
        );
    }

    fn signed_headers(caller: &'static str, token: &str, owner_user_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-local-connector-caller", HeaderValue::from_static(caller));
        headers.insert(
            "x-local-connector-internal-token",
            HeaderValue::from_str(token).expect("token header"),
        );
        headers.insert(
            "x-local-connector-owner-user-id",
            HeaderValue::from_str(owner_user_id).expect("owner header"),
        );
        headers
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            internal_mtls_port: 1,
            database_url: "postgresql://postgres:postgres@127.0.0.1/test".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            relay_request_timeout: Duration::from_secs(1),
            public_base_url: None,
            internal_api_secrets: HashMap::new(),
            require_device_connect_signature: true,
            device_connect_signature_max_skew: Duration::from_secs(300),
            active_session_lease_ttl: Duration::from_secs(90),
            valkey_url: "redis://127.0.0.1:6379/0".to_string(),
            valkey_key_prefix: "chatos:local-connector:test".to_string(),
            device_presence_ttl: Duration::from_secs(120),
            valkey_reconnect_delay: Duration::from_secs(2),
            relay_correlation_grace_ttl: Duration::from_secs(30),
            relay_delivery_ack_timeout: Duration::from_secs(3),
            terminal_subscriber_ttl: Duration::from_secs(60),
            terminal_subscriber_refresh_interval: Duration::from_secs(20),
            managed_requirements_toml_path: None,
            managed_requirements_signing_key_path: None,
            managed_requirements_signing_key_id: None,
            managed_requirements_bundle_ttl: Duration::from_secs(3600),
            controlled_network_signing_key_path: None,
            controlled_network_signing_key_id: None,
            controlled_network_policy_ttl: Duration::from_secs(300),
        }
    }
}
