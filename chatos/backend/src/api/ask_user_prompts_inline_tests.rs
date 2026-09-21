#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[test]
    fn detects_task_runner_cancelled_prompt_errors() {
        assert!(is_task_runner_prompt_cancelled_error(
            "Task Runner request failed: 400 Bad Request {\"error\":\"提示当前状态不允许提交: cancelled\"}",
        ));
        assert!(is_task_runner_prompt_cancelled_error(
            "Task Runner request failed: 400 Bad Request {\"error\":\"提示当前状态不允许取消: canceled\"}",
        ));
    }

    #[test]
    fn does_not_treat_unrelated_task_runner_errors_as_cancelled_prompts() {
        assert!(!is_task_runner_prompt_cancelled_error(
            "Task Runner request failed: 500 Internal Server Error",
        ));
        assert!(!is_task_runner_prompt_cancelled_error(
            "Task Runner request failed: 400 Bad Request {\"error\":\"提示当前状态不允许提交: submitted\"}",
        ));
    }

    #[test]
    fn detects_task_runner_missing_prompt_errors_as_stale() {
        assert!(is_task_runner_prompt_stale_error(
            "Task Runner request failed: 404 Not Found {\"error\":\"提示不存在: prompt-1\"}",
        ));
        assert!(is_task_runner_prompt_stale_error(
            "Task Runner request failed: 404 Not Found {\"error\":\"prompt not found\"}",
        ));
    }

    #[test]
    fn does_not_treat_arbitrary_not_found_errors_as_stale_prompts() {
        assert!(!is_task_runner_prompt_stale_error(
            "Task Runner request failed: 404 Not Found {\"error\":\"task not found\"}",
        ));
    }

    #[test]
    fn maps_task_runner_prompt_status_from_remote_value() {
        assert_eq!(
            task_runner_prompt_status_from_value(&json!({ "status": "cancelled" })),
            Some(AskUserPromptStatus::Canceled),
        );
        assert_eq!(
            task_runner_prompt_status_from_value(&json!({ "status": "submitted" })),
            Some(AskUserPromptStatus::Ok),
        );
        assert_eq!(
            task_runner_prompt_status_from_value(&json!({ "status": "pending" })),
            Some(AskUserPromptStatus::Pending),
        );
    }

    #[test]
    fn normalizes_empty_remote_prompt_response_status_to_fallback() {
        let response = task_runner_prompt_response_from_value(
            &json!({
                "status": "cancelled",
                "response": { "status": "pending", "reason": "run cancelled" }
            }),
            AskUserPromptStatus::Canceled,
        )
        .expect("response");

        assert_eq!(response.status, "canceled");
        assert_eq!(response.reason.as_deref(), Some("run cancelled"));
    }

    #[test]
    fn companion_prompt_removes_internal_and_external_execution_ids() {
        let record = AskUserPromptRecord {
            id: "prompt-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            conversation_turn_id: "turn-1".to_string(),
            tool_call_id: Some("tool-call-secret".to_string()),
            kind: "form".to_string(),
            status: AskUserPromptStatus::Pending,
            prompt: json!({
                "prompt_id": "prompt-1",
                "conversation_id": "conversation-1",
                "conversation_turn_id": "turn-1",
                "tool_call_id": "tool-call-secret",
                "kind": "form",
                "title": "Choose",
                "message": "Select one",
                "allow_cancel": true,
                "payload": { "fields": [{ "key": "answer", "label": "Answer" }] }
            }),
            response: Some(json!({ "values": { "answer": "private" } })),
            expires_at: Some("2026-09-14T00:00:00Z".to_string()),
            source: "task_runner".to_string(),
            external_prompt_id: Some("external-prompt".to_string()),
            external_task_id: Some("external-task".to_string()),
            external_run_id: Some("external-run".to_string()),
            external_project_id: Some("external-project".to_string()),
            created_at: "2026-09-14T00:00:00Z".to_string(),
            updated_at: "2026-09-14T00:00:00Z".to_string(),
        };

        let safe = companion_prompt_record(record);
        assert_eq!(safe["prompt"]["title"], "Choose");
        assert_eq!(safe["prompt"]["payload"]["fields"][0]["key"], "answer");
        for forbidden in [
            "tool_call_id",
            "response",
            "source",
            "external_prompt_id",
            "external_task_id",
            "external_run_id",
            "external_project_id",
        ] {
            assert!(safe.get(forbidden).is_none(), "must remove {forbidden}");
        }
        assert!(safe["prompt"].get("tool_call_id").is_none());
    }

    #[test]
    fn companion_response_removes_raw_task_runner_payload() {
        let response = (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "task_runner_prompt": { "project_id": "private-project" }
            })),
        );
        let (_, Json(safe)) = sanitize_companion_response(response, true);
        assert!(safe.get("task_runner_prompt").is_none());
    }

    #[tokio::test]
    async fn ask_user_mutations_reject_oversized_request_bodies() {
        let mut request = Request::post("/api/ask-user-prompts/prompt-1/submit")
            .header("content-type", "application/json")
            .body(Body::from(format!(
                "{{\"conversation_id\":\"conversation-1\",\"values\":{{\"answer\":\"{}\"}}}}",
                "a".repeat(ASK_USER_REQUEST_BODY_LIMIT_BYTES)
            )))
            .expect("request");
        request.extensions_mut().insert(AuthUser {
            user_id: "user-1".to_string(),
            role: "user".to_string(),
        });

        let response = router().oneshot(request).await.expect("route response");
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
