// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use axum::{body::Body, http::Request, response::Response, routing::any, Router};
use std::sync::{Arc, Mutex};

const PASSWORD: &str = "test-only-redirect-password";
const TOKEN: &str = "test-only-redirect-token";
const USER_PASSWORD: &str = "test-only-chatos-user-password";

#[derive(Debug)]
struct CapturedRequest {
    path: String,
    authorization: Option<String>,
    body: Vec<u8>,
}

type Captures = Arc<Mutex<Vec<CapturedRequest>>>;

struct Server {
    url: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn capture(request: Request<Body>, captures: Captures) {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, 4096).await.unwrap();
    captures.lock().unwrap().push(CapturedRequest {
        path: parts.uri.path().to_string(),
        authorization: parts
            .headers
            .get("Authorization")
            .map(|value| value.to_str().unwrap().to_string()),
        body: body.to_vec(),
    });
}

fn token_response() -> Response {
    Response::builder()
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({"access_token": TOKEN}).to_string(),
        ))
        .unwrap()
}

async fn serve(listener: tokio::net::TcpListener, router: Router) -> Server {
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Server { url, task }
}

async fn assert_redirect_blocked(status: StatusCode, same_origin: bool, body: Option<Value>) {
    let destination_requests = Captures::default();
    let captures = destination_requests.clone();
    let destination = serve(
        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(),
        Router::new().fallback(any(move |request| {
            let captures = captures.clone();
            async move {
                capture(request, captures).await;
                token_response()
            }
        })),
    )
    .await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let location = if same_origin {
        "/redirected".to_string()
    } else {
        format!("{}/redirected", destination.url)
    };
    let original_requests = Captures::default();
    let captures = original_requests.clone();
    let same_origin_captures = destination_requests.clone();
    let source = serve(
        listener,
        Router::new()
            .route(
                "/api",
                any(move |request| {
                    let captures = captures.clone();
                    let location = location.clone();
                    async move {
                        capture(request, captures).await;
                        Response::builder()
                            .status(status)
                            .header("Location", location)
                            // Redirect content must not trigger account-conflict fallback.
                            .body(Body::from(format!("duplicate {PASSWORD} {TOKEN}")))
                            .unwrap()
                    }
                }),
            )
            .route(
                "/redirected",
                any(move |request| {
                    let captures = same_origin_captures.clone();
                    async move {
                        capture(request, captures).await;
                        token_response()
                    }
                }),
            ),
    )
    .await;
    // Use the production client factory and send/decode boundaries. No real
    // Harness, database, process environment changes or production secrets.
    let client = build_harness_client_with_timeout(2000).unwrap();
    let request = if let Some(body) = &body {
        build_harness_request(
            &client,
            Method::POST,
            &format!("{}/api", source.url),
            None,
            Some(body),
        )
    } else {
        build_harness_request::<()>(
            &client,
            Method::GET,
            &format!("{}/api", source.url),
            Some(TOKEN),
            None,
        )
    };
    let response = send_harness_request(request).await.unwrap();
    {
        let originals = original_requests.lock().unwrap();
        assert_eq!(originals.len(), 1);
        assert_eq!(originals[0].path, "/api");
        if let Some(body) = body {
            assert_eq!(
                serde_json::from_slice::<Value>(&originals[0].body).unwrap(),
                body
            );
            assert!(originals[0].authorization.is_none());
        } else {
            assert_eq!(
                originals[0].authorization.as_deref(),
                Some(format!("Bearer {TOKEN}").as_str())
            );
            assert!(originals[0].body.is_empty());
        }
        let forwarded = destination_requests.lock().unwrap();
        assert!(
            forwarded.is_empty(),
            "{status} forwarded {} request(s); password in redirected body: {}",
            forwarded.len(),
            forwarded
                .iter()
                .any(|request| String::from_utf8_lossy(&request.body).contains(PASSWORD))
        );
    }
    assert_eq!(response.status(), status);
    let error = decode_harness_response::<HarnessTokenResponse>(response)
        .await
        .unwrap_err();
    assert_eq!(error.status, Some(status));
    assert!(!error.is_already_exists());
    for output in [
        error.to_string(),
        format!("{error:?}"),
        truncate_error(&error.to_string()),
    ] {
        assert!(!output.contains(PASSWORD));
        assert!(!output.contains(TOKEN));
        assert!(!output.contains("redirected"));
    }
}

async fn assert_password_redirect_blocked(status: StatusCode) {
    for same_origin in [false, true] {
        for body in [
            serde_json::to_value(HarnessRegisterRequest {
                uid: "test-user",
                email: "test@example.invalid",
                display_name: "test",
                password: PASSWORD,
            })
            .unwrap(),
            serde_json::to_value(HarnessLoginRequest {
                login_identifier: "test-user",
                password: PASSWORD,
            })
            .unwrap(),
        ] {
            assert_redirect_blocked(status, same_origin, Some(body)).await;
        }
    }
}

#[tokio::test]
async fn temporary_redirect_does_not_forward_harness_passwords() {
    assert_password_redirect_blocked(StatusCode::TEMPORARY_REDIRECT).await;
}

#[tokio::test]
async fn permanent_redirect_does_not_forward_harness_passwords() {
    assert_password_redirect_blocked(StatusCode::PERMANENT_REDIRECT).await;
}

#[tokio::test]
async fn redirects_do_not_forward_authenticated_harness_requests() {
    for status in [
        StatusCode::MOVED_PERMANENTLY,
        StatusCode::FOUND,
        StatusCode::SEE_OTHER,
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::PERMANENT_REDIRECT,
    ] {
        for same_origin in [false, true] {
            assert_redirect_blocked(status, same_origin, None).await;
        }
    }
}

#[tokio::test]
async fn direct_harness_response_still_succeeds() {
    let server = serve(
        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(),
        Router::new().fallback(any(|| async { token_response() })),
    )
    .await;
    let response = send_harness_request(
        build_harness_client_with_timeout(2000)
            .unwrap()
            .get(&server.url),
    )
    .await
    .unwrap();
    let token = decode_harness_response::<HarnessTokenResponse>(response)
        .await
        .unwrap();
    assert_eq!(token.access_token, TOKEN);
}

#[tokio::test]
async fn direct_harness_request_bodies_use_only_the_provisioning_credential() {
    let captures = Captures::default();
    let server_captures = captures.clone();
    let server = serve(
        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(),
        Router::new().fallback(any(move |request| {
            let captures = server_captures.clone();
            async move {
                capture(request, captures).await;
                token_response()
            }
        })),
    )
    .await;
    let credential = generated_harness_provisioning_password();
    let client = build_harness_client_with_timeout(2000).unwrap();
    let requests = [
        (
            "/api/v1/register",
            serde_json::to_value(HarnessRegisterRequest {
                uid: "test-user",
                email: "test@example.invalid",
                display_name: "test",
                password: credential.as_str(),
            })
            .unwrap(),
        ),
        (
            "/api/v1/login",
            serde_json::to_value(HarnessLoginRequest {
                login_identifier: "test-user",
                password: credential.as_str(),
            })
            .unwrap(),
        ),
    ];

    for (path, body) in requests {
        let response = send_harness_request(build_harness_request(
            &client,
            Method::POST,
            &format!("{}{path}", server.url),
            None,
            Some(&body),
        ))
        .await
        .unwrap();
        decode_harness_response::<HarnessTokenResponse>(response)
            .await
            .unwrap();
    }

    let captures = captures.lock().unwrap();
    assert_eq!(captures.len(), 2);
    for request in captures.iter() {
        let body = String::from_utf8(request.body.clone()).unwrap();
        assert!(!body.contains(USER_PASSWORD));
        assert!(body.contains(credential.as_str()));
        assert!(request.authorization.is_none());
    }
    assert_eq!(captures[0].path, "/api/v1/register");
    assert_eq!(captures[1].path, "/api/v1/login");
}
