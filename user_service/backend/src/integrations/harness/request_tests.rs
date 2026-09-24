// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

const PASSWORD: &str = "test-only-transport-password";
const TOKEN: &str = "test-only-transport-token";

fn assert_safe_send_failure(error: &HarnessRequestError) {
    // These are the representations used by diagnostics and the provisioning
    // failure path before saving last_error. No endpoint or source text belongs
    // in any of them, even when it resembles an account-conflict response.
    for output in [
        error.to_string(),
        format!("{error:?}"),
        truncate_error(&format!("create or login harness user failed: {error}")),
    ] {
        assert!(!output.contains(PASSWORD), "request password escaped");
        assert!(!output.contains(TOKEN), "request token escaped");
        assert!(!output.contains("example.invalid"), "request URL escaped");
    }
    assert_eq!(error.status, None);
    assert!(
        !error.is_already_exists(),
        "send failure triggered login fallback"
    );
}

#[tokio::test]
async fn unsupported_endpoint_does_not_expose_url_secrets() {
    // An unsupported scheme deterministically fails without any network I/O.
    let endpoint =
        format!("harness-test://example.invalid/{PASSWORD}?access_token={TOKEN}&reason=duplicate");
    let request = reqwest::Client::new().get(endpoint);
    let raw_error = request.try_clone().unwrap().send().await.unwrap_err();
    assert!(raw_error.to_string().contains(PASSWORD));
    assert!(raw_error.to_string().contains(TOKEN));

    let error = send_harness_request(request).await.unwrap_err();
    assert_safe_send_failure(&error);
}

#[tokio::test]
async fn builder_failure_does_not_trigger_existing_account_fallback() {
    struct InvalidBody;

    impl Serialize for InvalidBody {
        fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom(format!(
                "duplicate {PASSWORD} {TOKEN}"
            )))
        }
    }

    // The JSON builder fails before sending. Source error text is untrusted
    // and must not control the register-to-login fallback decision.
    let request = reqwest::Client::new()
        .post("https://example.invalid/api/v1/register")
        .json(&InvalidBody);
    let error = send_harness_request(request).await.unwrap_err();
    assert_safe_send_failure(&error);
}

#[tokio::test]
async fn timeout_does_not_expose_url_secrets() {
    // Hold a loopback listener without serving a response. No external service,
    // environment mutation or production credentials are needed.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/{PASSWORD}?access_token={TOKEN}&reason=duplicate",
        listener.local_addr().unwrap()
    );
    let request = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(endpoint)
        .timeout(std::time::Duration::from_millis(100));
    let raw_error = request.try_clone().unwrap().send().await.unwrap_err();
    assert!(raw_error.is_timeout());
    assert!(raw_error.to_string().contains(PASSWORD));
    let error = send_harness_request(request).await.unwrap_err();
    assert_safe_send_failure(&error);
}
