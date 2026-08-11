// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod retry_tests {
    use axum::http::StatusCode;

    use super::{require_retryable_message_run, TaskRunStatus};

    #[test]
    fn message_run_retry_accepts_failed_and_blocked_nodes() {
        for status in [TaskRunStatus::Failed, TaskRunStatus::Blocked] {
            require_retryable_message_run(&status)
                .expect("terminal problem run should be retryable");
        }
        for status in [
            TaskRunStatus::Queued,
            TaskRunStatus::Running,
            TaskRunStatus::Succeeded,
            TaskRunStatus::Cancelled,
        ] {
            let error = require_retryable_message_run(&status)
                .expect_err("non-retryable run must not be retried from a message task card");
            assert_eq!(error.status, StatusCode::BAD_REQUEST);
        }
    }
}

mod plugin_projection_tests {
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    use super::{
        paginate_run_events, trim_run_for_chatos_detail, TaskRunRecord,
        RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES,
    };
    use crate::models::TaskRunEventRecord;

    #[test]
    fn chatos_run_snapshot_replaces_plugin_command_arguments_with_hashes() {
        let command_arguments = "检查 src/private.rs access_token=do-not-display";
        let expected_sha256 = hex::encode(Sha256::digest(command_arguments.as_bytes()));
        let run = TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({
                "plugin_config": {
                    "device_id": "device-1",
                    "workspace_id": "workspace-1",
                    "selected_plugins": [{
                        "plugin_id": "plugin-review",
                        "selected_command_ids": ["review"]
                    }],
                    "command_invocations": [{
                        "plugin_id": "plugin-review",
                        "command_id": "review",
                        "arguments": command_arguments
                    }]
                },
                "plugin_snapshots": [{
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "component_snapshots": [{
                        "component_key": "review",
                        "kind": "command",
                        "runtime": {
                            "runtime_kind": "markdown_command",
                            "arguments": command_arguments
                        }
                    }]
                }]
            }),
            Vec::new(),
            "2026-07-27T00:00:00Z".to_string(),
        );

        let projected = trim_run_for_chatos_detail(run).input_snapshot;
        let serialized = serde_json::to_string(&projected).expect("serialize projected snapshot");

        assert!(!serialized.contains(command_arguments));
        assert!(!serialized.contains("do-not-display"));
        assert_eq!(
            projected.pointer("/plugin_config/command_invocations/0/arguments_present"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            projected
                .pointer("/plugin_config/command_invocations/0/arguments_sha256")
                .and_then(Value::as_str),
            Some(expected_sha256.as_str())
        );
        assert_eq!(
            projected
                .pointer("/plugin_snapshots/0/component_snapshots/0/runtime/arguments_sha256")
                .and_then(Value::as_str),
            Some(expected_sha256.as_str())
        );
    }

    #[test]
    fn oversized_chatos_run_snapshot_retains_bounded_plugin_audit_summary() {
        let run = TaskRunRecord::queued(
            "run-oversized".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({
                "padding": "x".repeat(RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES + 1024),
                "plugin_config": {
                    "device_id": "device-1",
                    "selected_plugins": [{
                        "plugin_id": "plugin-review",
                        "selected_command_ids": ["review"]
                    }],
                    "command_invocations": [{
                        "plugin_id": "plugin-review",
                        "command_id": "review",
                        "arguments": "private-command-arguments"
                    }]
                },
                "plugin_snapshots": [{
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "version": "1.2.3",
                    "device_id": "must-not-be-copied-to-summary",
                    "component_snapshots": [{
                        "component_key": "review",
                        "kind": "command",
                        "content_sha256": "a".repeat(64),
                        "runtime": {"arguments": "private-command-arguments"}
                    }]
                }]
            }),
            Vec::new(),
            "2026-07-27T00:00:00Z".to_string(),
        );

        let projected = trim_run_for_chatos_detail(run).input_snapshot;
        let serialized = serde_json::to_string(&projected).expect("serialize projected snapshot");

        assert_eq!(
            projected.get("truncated").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            projected
                .pointer("/plugin_config/selected_plugins/0/plugin_id")
                .and_then(Value::as_str),
            Some("plugin-review")
        );
        assert_eq!(
            projected
                .pointer("/plugin_snapshots/0/component_snapshots/0/component_key")
                .and_then(Value::as_str),
            Some("review")
        );
        assert!(!serialized.contains("private-command-arguments"));
        assert!(!serialized.contains("must-not-be-copied-to-summary"));
    }

    #[test]
    fn chatos_plugin_events_are_projected_before_diagnostic_display() {
        let secret = "must-not-reach-chatos-plugin-display";
        let events = vec![
            event(
                "event-runtime",
                "plugin_runtime",
                json!({
                    "run_id": "run-1",
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "component_key": "review-hooks",
                    "adapter_session_id": "adapter-1",
                    "phase": "execute",
                    "status": "failed",
                    "operation": "dispatch_hook_event",
                    "tool_name": "browser_snapshot",
                    "duration_ms": 25,
                    "error": "approval declined",
                    "arguments": secret,
                    "tool_payload": {"content": secret},
                    "stdout": secret,
                    "stderr": secret,
                    "hook_dispatch": {
                        "event": "PreToolUse",
                        "snapshot_sha256": "a".repeat(64),
                        "blocking_failure": false,
                        "executions": [{
                            "hook_id": "private-hook-id",
                            "matched": true,
                            "succeeded": false,
                            "timed_out": false,
                            "workspace_write": true,
                            "workspace_write_approved": false,
                            "stdout_sha256": "b".repeat(64),
                            "stderr_sha256": "c".repeat(64),
                            "error": secret
                        }]
                    }
                }),
            ),
            event(
                "event-hook",
                "plugin_hook_blocked",
                json!({
                    "event": "PreToolUse",
                    "blocking_failure": true,
                    "tool_name": "browser_snapshot",
                    "tool_kind": "builtin",
                    "component_key": "review-hooks",
                    "summary_sha256": "d".repeat(64),
                    "raw_payload": secret
                }),
            ),
            event(
                "event-ui",
                "plugin_ui_ready",
                json!({
                    "event_schema_version": 1,
                    "run_id": "run-1",
                    "device_id": "device-secret",
                    "workspace_id": "workspace-secret",
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "artifact_sha256": "e".repeat(64),
                    "component_key": "workbench",
                    "adapter_session_id": "adapter-ui",
                    "ui": {
                        "title": "Review Workbench",
                        "surface": "workbench",
                        "snapshot_sha256": "f".repeat(64),
                        "bridge_protocol_version": 1,
                        "bridge_capabilities": ["host.context.read", "artifact.list"],
                        "artifact_mime_types": ["application/json"],
                        "relative_source_path": secret,
                        "assets": [{"relative_path": secret}],
                        "content_security_policy": secret
                    }
                }),
            ),
            event(
                "event-artifact",
                "plugin_artifact_ready",
                json!({
                    "event_schema_version": 1,
                    "artifact": {
                        "artifact_id": format!("pa_{}", "1".repeat(32)),
                        "owner": {
                            "owner_user_id": "owner-secret",
                            "run_id": "run-1",
                            "device_id": "device-secret",
                            "workspace_id": "workspace-secret",
                            "plugin_id": "plugin-review",
                            "release_id": "release-1",
                            "artifact_sha256": "e".repeat(64),
                            "component_key": "documents",
                            "adapter_session_id": "adapter-documents"
                        },
                        "workspace_relative_path": secret,
                        "display_name": "report.docx",
                        "media_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                        "size_bytes": 42,
                        "sha256": "2".repeat(64),
                        "created_at": "2026-07-27T00:00:00Z",
                        "producer_tool_name": "create_document",
                        "downloadable": true,
                        "mutable": false,
                        "body_base64": secret
                    }
                }),
            ),
        ];

        let (events, total, has_more) = paginate_run_events(events, 10, 0);
        let serialized = serde_json::to_string(&events).expect("serialize projected events");

        assert_eq!(total, 4);
        assert!(!has_more);
        assert!(serialized.len() < RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES);
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains("owner-secret"));
        assert!(!serialized.contains("device-secret"));
        assert!(!serialized.contains("workspace-secret"));
        assert!(!serialized.contains("stdout_sha256"));
        assert!(!serialized.contains("stderr_sha256"));
        assert!(!serialized.contains("body_base64"));
        assert_eq!(
            events[0]
                .payload
                .as_ref()
                .and_then(|payload| payload.pointer("/hook_dispatch/executions/0/matched")),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            events[2]
                .payload
                .as_ref()
                .and_then(|payload| payload.pointer("/ui/title"))
                .and_then(Value::as_str),
            Some("Review Workbench")
        );
        assert_eq!(
            events[3]
                .payload
                .as_ref()
                .and_then(|payload| payload.pointer("/artifact/display_name"))
                .and_then(Value::as_str),
            Some("report.docx")
        );
    }

    #[test]
    fn oversized_tool_results_keep_pairing_fields_and_a_visible_preview() {
        let events = vec![
            event(
                "event-tools-start",
                "tools_start",
                json!([{
                    "id": "call-large",
                    "type": "function",
                    "function": {
                        "name": "code_maintainer_read_read_file_raw",
                        "arguments": {"path": "src/large.rs"}
                    }
                }]),
            ),
            event(
                "event-tool-result",
                "tool_stream",
                json!({
                    "tool_call_id": "call-large",
                    "name": "code_maintainer_read_read_file_raw",
                    "success": true,
                    "is_error": false,
                    "is_stream": false,
                    "content": "x".repeat(RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES * 2),
                    "result": {
                        "path": "src/large.rs",
                        "content": "y".repeat(RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES * 2)
                    }
                }),
            ),
        ];

        let (events, total, has_more) = paginate_run_events(events, 10, 0);

        assert_eq!(total, 2);
        assert!(!has_more);
        assert_eq!(
            events[1]
                .payload
                .as_ref()
                .and_then(|payload| payload.get("tool_call_id"))
                .and_then(Value::as_str),
            Some("call-large")
        );
        assert_eq!(
            events[1]
                .payload
                .as_ref()
                .and_then(|payload| payload.pointer("/result/truncated")),
            Some(&Value::Bool(true))
        );
        assert!(events[1]
            .payload
            .as_ref()
            .and_then(|payload| payload.pointer("/result/preview"))
            .and_then(Value::as_str)
            .is_some_and(|preview| preview.contains("src/large.rs")));
    }

    fn event(id: &str, event_type: &str, payload: Value) -> TaskRunEventRecord {
        TaskRunEventRecord {
            id: id.to_string(),
            run_id: "run-1".to_string(),
            event_type: event_type.to_string(),
            message: Some(format!("{event_type} event")),
            payload: Some(payload),
            created_at: "2026-07-27T00:00:00Z".to_string(),
        }
    }
}
