// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) const HARNESS_REPO_WRITE_SCOPE: &str = "harness.repo.write";
pub(crate) const HARNESS_ACCESS_READ_SCOPE: &str = "harness.access.read";

pub(crate) fn signed_user_service_request(
    request: reqwest::RequestBuilder,
    internal_secret: &str,
    scope: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let internal_secret = internal_secret.trim();
    let token = chatos_service_runtime::issue_internal_service_token(
        internal_secret,
        "project-service",
        "user-service",
        scope,
        60,
    )?;
    Ok(request
        .header("X-User-Service-Caller", "project-service")
        .header("X-User-Service-Internal-Token", token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_user_service_request_uses_scoped_token_without_static_secret() {
        let request = signed_user_service_request(
            reqwest::Client::new().get("http://127.0.0.1:39190/api/internal/test"),
            "a-long-project-user-service-secret",
            HARNESS_ACCESS_READ_SCOPE,
        )
        .expect("signed request")
        .build()
        .expect("build request");
        assert_eq!(
            request
                .headers()
                .get("x-user-service-caller")
                .and_then(|value| value.to_str().ok()),
            Some("project-service")
        );
        let token = request
            .headers()
            .get("x-user-service-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("internal token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "a-long-project-user-service-secret",
            "project-service",
            "user-service",
            HARNESS_ACCESS_READ_SCOPE,
        )
        .expect("valid token");
        assert!(!request
            .headers()
            .contains_key("x-user-service-internal-secret"));
    }
}
