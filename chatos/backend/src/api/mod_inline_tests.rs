#[cfg(test)]
mod tests {
    use super::{
        companion_conversation_read_path, companion_task_read_path, enforce_client_scope,
        internal_router, plugin_ui_resource_namespace_allowed,
        remove_plugin_ui_resource_cors_headers, sanitize_request_uri, websocket_auth_from_query,
        WebSocketQueryAuth,
    };
    use crate::core::auth::{AuthHeaderError, AuthUser};
    use crate::core::websocket_ticket::issue_websocket_ticket;
    use axum::body::Body;
    use axum::http::{header::UPGRADE, HeaderMap, HeaderValue, Method, Request, Uri};
    use tower::ServiceExt;

    fn websocket_request(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header(UPGRADE, "websocket")
            .body(Body::empty())
            .expect("build websocket request")
    }

    fn auth_user() -> AuthUser {
        AuthUser {
            user_id: "user_1".to_string(),
            role: "user".to_string(),
        }
    }

    #[test]
    fn wechat_companion_scope_allows_only_conversation_control_surface() {
        let scopes = vec!["wechat_companion".to_string()];
        for (method, path) in [
            (Method::GET, "/api/companion/conversations/c1"),
            (
                Method::GET,
                "/api/companion/conversations/c1/compact-history",
            ),
            (Method::GET, "/api/companion/conversations/c1/state"),
            (Method::GET, "/api/companion/messages/m1/tasks"),
            (Method::GET, "/api/realtime/ws"),
            (Method::POST, "/api/agent/chat/send"),
            (Method::POST, "/api/agent/chat/guidance"),
            (Method::POST, "/api/agent/chat/stop"),
            (Method::POST, "/api/auth/ws-ticket"),
        ] {
            assert!(enforce_client_scope(&method, path, scopes.as_slice()).is_ok());
        }
        for (method, path) in [
            (Method::GET, "/api/companion/conversations"),
            (Method::POST, "/api/conversations"),
            (Method::GET, "/api/conversations"),
            (Method::GET, "/api/conversations/c1/runtime-settings"),
            (
                Method::GET,
                "/api/companion/conversations/c1/runtime-settings",
            ),
            (Method::GET, "/api/companion/messages/m1/tasks/extra"),
            (Method::GET, "/api/fs/list"),
            (Method::GET, "/api/terminals/terminal-1/ws"),
            (Method::POST, "/api/terminals"),
            (Method::GET, "/api/model-configs"),
        ] {
            assert!(enforce_client_scope(&method, path, scopes.as_slice()).is_err());
        }
        assert!(enforce_client_scope(
            &Method::POST,
            "/api/terminals",
            &["user_service".to_string()],
        )
        .is_ok());
    }

    #[test]
    fn companion_conversation_paths_are_matched_by_complete_segments() {
        for path in [
            "/api/companion/conversations/c1",
            "/api/companion/conversations/c1/compact-history",
            "/api/companion/conversations/c1/state",
        ] {
            assert!(companion_conversation_read_path(path), "path={path}");
        }
        for path in [
            "/api/companion/conversations",
            "/api/companion/conversations/",
            "/api/companion/conversations/c1/runtime-settings",
            "/api/companion/conversations/c1/state/extra",
            "/api/companion/conversations-malicious",
        ] {
            assert!(!companion_conversation_read_path(path), "path={path}");
        }
    }

    #[test]
    fn companion_task_paths_are_matched_by_complete_segments() {
        assert!(companion_task_read_path("/api/companion/messages/m1/tasks"));
        for path in [
            "/api/companion/messages/m1",
            "/api/companion/messages//tasks",
            "/api/companion/messages/m1/tasks/extra",
            "/api/companion/messages-malicious/m1/tasks",
        ] {
            assert!(!companion_task_read_path(path), "path={path}");
        }
    }

    #[test]
    fn sanitize_request_uri_redacts_sensitive_query_values() {
        let uri: Uri = "/api/realtime/ws?ws_ticket=ticket_1&access_token=token_1&verification_code=123456&plain=value"
            .parse()
            .expect("parse uri");
        assert_eq!(
            sanitize_request_uri(&uri),
            "/api/realtime/ws?ws_ticket=[redacted]&access_token=[redacted]&verification_code=[redacted]&plain=value"
        );
    }

    #[test]
    fn sanitize_request_uri_redacts_plugin_ui_workbench_session_paths() {
        let session_id = format!("pui_{}", "a".repeat(64));
        let uri: Uri = format!("/api/plugin-ui/workbench/{session_id}/ui/index.html?plain=value")
            .parse()
            .expect("parse uri");
        assert_eq!(
            sanitize_request_uri(&uri),
            "/api/plugin-ui/workbench/[redacted]/ui/index.html?plain=value"
        );
    }

    #[test]
    fn plugin_ui_resource_origin_is_an_exact_get_only_namespace() {
        let origin = Some("https://plugin-ui.example.com");
        assert!(plugin_ui_resource_namespace_allowed(
            origin,
            &Method::GET,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("plugin-ui.example.com"),
        ));
        assert!(plugin_ui_resource_namespace_allowed(
            origin,
            &Method::HEAD,
            "/api/plugin-ui/workbench/pui_session/ui/app.js",
            Some("plugin-ui.example.com:443"),
        ));
        assert!(!plugin_ui_resource_namespace_allowed(
            origin,
            &Method::POST,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("plugin-ui.example.com"),
        ));
        assert!(!plugin_ui_resource_namespace_allowed(
            origin,
            &Method::GET,
            "/api/sessions",
            Some("plugin-ui.example.com"),
        ));
        assert!(!plugin_ui_resource_namespace_allowed(
            origin,
            &Method::GET,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("app.example.com"),
        ));
        assert!(plugin_ui_resource_namespace_allowed(
            None,
            &Method::GET,
            "/api/plugin-ui/workbench/pui_session/ui/index.html",
            Some("app.example.com"),
        ));

        let mut headers = HeaderMap::new();
        headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        headers.insert(
            "access-control-allow-credentials",
            HeaderValue::from_static("true"),
        );
        headers.insert("content-type", HeaderValue::from_static("text/html"));
        remove_plugin_ui_resource_cors_headers(&mut headers);
        assert!(!headers.contains_key("access-control-allow-origin"));
        assert!(!headers.contains_key("access-control-allow-credentials"));
        assert_eq!(headers["content-type"], "text/html");
    }

    #[test]
    fn websocket_auth_from_query_accepts_ws_ticket() {
        let ticket = issue_websocket_ticket(
            "access_token_1",
            &auth_user(),
            &["wechat_companion".to_string()],
        )
        .expect("issue websocket ticket");
        let request =
            websocket_request(format!("/api/realtime/ws?ws_ticket={}", ticket.ticket).as_str());

        let result = websocket_auth_from_query(&request).expect("resolve websocket auth");
        match result {
            WebSocketQueryAuth::Ticket(record) => {
                assert_eq!(record.access_token, "access_token_1");
                assert_eq!(record.auth_user.user_id, "user_1");
                assert_eq!(record.scopes, vec!["wechat_companion"]);
            }
        }
    }

    #[test]
    fn websocket_auth_from_query_rejects_legacy_access_token_param() {
        let request = websocket_request("/api/realtime/ws?access_token=legacy_token");
        let error = websocket_auth_from_query(&request).expect_err("legacy query token rejected");
        assert_eq!(error, AuthHeaderError::MissingAuthorization);
    }

    #[tokio::test]
    async fn internal_mtls_router_does_not_expose_metrics_endpoint() {
        let response = internal_router()
            .oneshot(
                Request::get("/metrics")
                    .body(Body::empty())
                    .expect("build metrics request"),
            )
            .await
            .expect("route metrics request");
        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    }
}
