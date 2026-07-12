// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

const PROJECT_SERVICE_SYNC_SECRET_HEADER: &str = "x-project-service-sync-secret";
const PROJECT_SERVICE_CALLER_HEADER: &str = "x-project-service-caller";
const PROJECT_SERVICE_TOKEN_HEADER: &str = "x-project-service-internal-token";
const PROJECT_SERVICE_SCOPE_HEADER: &str = "x-project-service-internal-scope";
const PROJECT_SERVICE_TOKEN_AUDIENCE: &str = "project-service";
const LOCAL_CONNECTOR_SECRET_HEADER: &str = "x-local-connector-internal-secret";
const LOCAL_CONNECTOR_CALLER_HEADER: &str = "x-local-connector-caller";
const LOCAL_CONNECTOR_TOKEN_HEADER: &str = "x-local-connector-internal-token";
const LOCAL_CONNECTOR_SCOPE_HEADER: &str = "x-local-connector-internal-scope";
const LOCAL_CONNECTOR_TOKEN_AUDIENCE: &str = "local-connector-service";
const SANDBOX_MANAGER_SECRET_HEADER: &str = "x-sandbox-client-key";
const SANDBOX_MANAGER_CALLER_HEADER: &str = "x-sandbox-caller";
const SANDBOX_MANAGER_TOKEN_HEADER: &str = "x-sandbox-internal-token";
const SANDBOX_MANAGER_SCOPE_HEADER: &str = "x-sandbox-internal-scope";
const SANDBOX_MANAGER_TOKEN_AUDIENCE: &str = "sandbox-manager";

pub fn prepare_http_headers(
    headers: &HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    if let Some(scope) = header_value(headers, SANDBOX_MANAGER_SCOPE_HEADER) {
        return sign_headers(
            headers,
            HeaderSigningProfile {
                caller_header: SANDBOX_MANAGER_CALLER_HEADER,
                secret_header: SANDBOX_MANAGER_SECRET_HEADER,
                token_header: SANDBOX_MANAGER_TOKEN_HEADER,
                scope_header: SANDBOX_MANAGER_SCOPE_HEADER,
                audience: SANDBOX_MANAGER_TOKEN_AUDIENCE,
                service_label: "Sandbox Manager",
                extra_private_headers: &["x-sandbox-client-id"],
            },
            scope,
        );
    }
    if let Some(scope) = header_value(headers, LOCAL_CONNECTOR_SCOPE_HEADER) {
        return sign_headers(
            headers,
            HeaderSigningProfile {
                caller_header: LOCAL_CONNECTOR_CALLER_HEADER,
                secret_header: LOCAL_CONNECTOR_SECRET_HEADER,
                token_header: LOCAL_CONNECTOR_TOKEN_HEADER,
                scope_header: LOCAL_CONNECTOR_SCOPE_HEADER,
                audience: LOCAL_CONNECTOR_TOKEN_AUDIENCE,
                service_label: "Local Connector",
                extra_private_headers: &[],
            },
            scope,
        );
    }
    let Some(scope) = header_value(headers, PROJECT_SERVICE_SCOPE_HEADER) else {
        return Ok(headers.clone());
    };
    sign_headers(
        headers,
        HeaderSigningProfile {
            caller_header: PROJECT_SERVICE_CALLER_HEADER,
            secret_header: PROJECT_SERVICE_SYNC_SECRET_HEADER,
            token_header: PROJECT_SERVICE_TOKEN_HEADER,
            scope_header: PROJECT_SERVICE_SCOPE_HEADER,
            audience: PROJECT_SERVICE_TOKEN_AUDIENCE,
            service_label: "project service",
            extra_private_headers: &[],
        },
        scope,
    )
}

struct HeaderSigningProfile<'a> {
    caller_header: &'a str,
    secret_header: &'a str,
    token_header: &'a str,
    scope_header: &'a str,
    audience: &'a str,
    service_label: &'a str,
    extra_private_headers: &'a [&'a str],
}

fn sign_headers(
    headers: &HashMap<String, String>,
    profile: HeaderSigningProfile<'_>,
    scope: &str,
) -> Result<HashMap<String, String>, String> {
    let caller = header_value(headers, profile.caller_header).ok_or_else(|| {
        format!(
            "{} caller is required for MCP request signing",
            profile.service_label
        )
    })?;
    let secret = header_value(headers, profile.secret_header).ok_or_else(|| {
        format!(
            "{} internal secret is required for MCP request signing",
            profile.service_label
        )
    })?;
    let token = chatos_service_runtime::issue_internal_service_token(
        secret,
        caller,
        profile.audience,
        scope,
        60,
    )?;
    let mut prepared = headers
        .iter()
        .filter(|(key, _)| {
            !key.eq_ignore_ascii_case(profile.scope_header)
                && !key.eq_ignore_ascii_case(profile.token_header)
                && !key.eq_ignore_ascii_case(profile.secret_header)
                && !profile
                    .extra_private_headers
                    .iter()
                    .any(|private| key.eq_ignore_ascii_case(private))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<HashMap<_, _>>();
    prepared.insert(profile.token_header.to_string(), token);
    Ok(prepared)
}

fn header_value<'a>(headers: &'a HashMap<String, String>, expected_key: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(expected_key))
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_service_headers_receive_fresh_scoped_token() {
        assert_signed_headers(
            HashMap::from([
                (
                    PROJECT_SERVICE_SYNC_SECRET_HEADER.to_string(),
                    "a-long-project-service-secret".to_string(),
                ),
                (
                    PROJECT_SERVICE_CALLER_HEADER.to_string(),
                    "task-runner".to_string(),
                ),
                (
                    PROJECT_SERVICE_SCOPE_HEADER.to_string(),
                    "project.harness".to_string(),
                ),
                (
                    PROJECT_SERVICE_TOKEN_HEADER.to_string(),
                    "stale-token".to_string(),
                ),
            ]),
            PROJECT_SERVICE_SYNC_SECRET_HEADER,
            PROJECT_SERVICE_TOKEN_HEADER,
            "a-long-project-service-secret",
            "task-runner",
            PROJECT_SERVICE_TOKEN_AUDIENCE,
            "project.harness",
        );
    }

    #[test]
    fn local_connector_headers_receive_fresh_scoped_token() {
        assert_signed_headers(
            HashMap::from([
                (
                    LOCAL_CONNECTOR_SECRET_HEADER.to_string(),
                    "a-long-task-runner-local-connector-secret".to_string(),
                ),
                (
                    LOCAL_CONNECTOR_CALLER_HEADER.to_string(),
                    "task-runner".to_string(),
                ),
                (
                    LOCAL_CONNECTOR_SCOPE_HEADER.to_string(),
                    "relay.mcp".to_string(),
                ),
                (
                    LOCAL_CONNECTOR_TOKEN_HEADER.to_string(),
                    "stale-token".to_string(),
                ),
            ]),
            LOCAL_CONNECTOR_SECRET_HEADER,
            LOCAL_CONNECTOR_TOKEN_HEADER,
            "a-long-task-runner-local-connector-secret",
            "task-runner",
            LOCAL_CONNECTOR_TOKEN_AUDIENCE,
            "relay.mcp",
        );
    }

    #[test]
    fn sandbox_manager_headers_receive_token_without_client_key() {
        assert_signed_headers(
            HashMap::from([
                (
                    SANDBOX_MANAGER_SECRET_HEADER.to_string(),
                    "a-long-project-sandbox-secret".to_string(),
                ),
                (
                    SANDBOX_MANAGER_CALLER_HEADER.to_string(),
                    "project-service".to_string(),
                ),
                (
                    SANDBOX_MANAGER_SCOPE_HEADER.to_string(),
                    "sandbox.service".to_string(),
                ),
            ]),
            SANDBOX_MANAGER_SECRET_HEADER,
            SANDBOX_MANAGER_TOKEN_HEADER,
            "a-long-project-sandbox-secret",
            "project-service",
            SANDBOX_MANAGER_TOKEN_AUDIENCE,
            "sandbox.service",
        );
    }

    fn assert_signed_headers(
        headers: HashMap<String, String>,
        secret_header: &str,
        token_header: &str,
        secret: &str,
        caller: &str,
        audience: &str,
        scope: &str,
    ) {
        let prepared = prepare_http_headers(&headers).expect("prepare signed headers");
        assert!(!prepared
            .keys()
            .any(|key| key.eq_ignore_ascii_case(secret_header)));
        let token = prepared
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(token_header))
            .map(|(_, value)| value.as_str())
            .expect("token");
        assert_ne!(token, "stale-token");
        chatos_service_runtime::verify_internal_service_token(
            token, secret, caller, audience, scope,
        )
        .expect("valid token");
    }
}
