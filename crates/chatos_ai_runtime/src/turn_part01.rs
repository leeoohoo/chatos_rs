#[cfg(all(test, feature = "local-agent-loop"))]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{json, Value};

    use super::{
        build_contextual_input, enable_openai_responses_protocol, input_value_to_items,
        user_text_item, ContextualTurnRequest, RuntimeTurnSpec,
    };
    use crate::{
        AiRuntime, AiRuntimeOptions, AiTurnStatus, MemoryContextComposer, MemoryScope,
        ModelRequest, ModelRuntimeConfig, RuntimeRecordOptions, SaveRecordInput,
    };

    #[tokio::test]
    async fn build_contextual_input_orders_prefix_memory_and_current_items() {
        let input = build_contextual_input(
            None,
            None,
            &[json!({"role":"system","content":"prefix"})],
            &[json!({"role":"user","content":"current"})],
            json!("fallback"),
            None,
        )
        .await
        .expect("contextual input");

        let items = input.as_array().expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].get("content").and_then(Value::as_str),
            Some("prefix")
        );
        assert_eq!(
            items[1].get("content").and_then(Value::as_str),
            Some("current")
        );
    }

    #[tokio::test]
    async fn build_contextual_input_uses_fallback_when_current_is_empty() {
        let input = build_contextual_input(None, None, &[], &[], json!("fallback"), None)
            .await
            .expect("contextual input");

        let items = input.as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("content").and_then(Value::as_str),
            Some("fallback")
        );
    }

    #[tokio::test]
    async fn durable_responses_history_remains_an_immutable_cache_prefix() {
        let original = json!({"role":"user","content":"implement inventory cli"});
        let reasoning = json!({"type":"reasoning","id":"rs-1","summary":[]});
        let call = json!({"type":"function_call","id":"fc-1","call_id":"call-1","name":"read_file","arguments":"{}"});
        let output = json!({"type":"function_call_output","call_id":"call-1","output":"README"});
        let durable = vec![
            original.clone(),
            reasoning.clone(),
            call.clone(),
            output.clone(),
        ];

        let input = build_contextual_input(
            None,
            None,
            &[json!({"role":"system","content":"stable prompt"})],
            durable.as_slice(),
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("contextual input");
        let items = input.as_array().expect("items");

        assert_eq!(items, durable.as_slice());
    }

    #[tokio::test]
    async fn durable_history_is_not_recomposed_with_memory_context() {
        async fn compose() -> Json<Value> {
            Json(json!({
                "thread_id": "thread-1",
                "blocks": [{"block_type": "thread_summary_top_level", "text": "summary"}],
                "recent_records": [],
                "meta": {"summary_count": 1, "recent_record_count": 0}
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind memory engine mock");
        let address = listener.local_addr().expect("memory engine mock address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                Router::new().route("/api/memory-engine/v1/context/compose", post(compose)),
            )
            .await;
        });
        let composer = MemoryContextComposer::new_direct(
            format!("http://{address}"),
            Duration::from_secs(1),
            "task_runner",
        )
        .expect("memory composer");
        let scope = MemoryScope::thread("tenant-1", "task_runner", "thread-1");
        let durable = vec![
            json!({"role":"user","content":"implement inventory cli"}),
            json!({"type":"reasoning","id":"rs-1","summary":[]}),
            json!({"type":"function_call","id":"fc-old","call_id":"call-old","name":"read_file","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"call-old","output":"old README"}),
            json!({"type":"reasoning","id":"rs-2","summary":[]}),
            json!({"type":"function_call","id":"fc-new","call_id":"call-new","name":"run_tests","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"call-new","output":"cargo test"}),
        ];

        let input = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[json!({"role":"system","content":"stable prompt"})],
            durable.as_slice(),
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("contextual input");
        server.abort();

        let items = input.as_array().expect("items");
        assert_eq!(items, durable.as_slice());
        assert_eq!(
            items
                .iter()
                .filter(|item| item.get("role").and_then(Value::as_str) == Some("user"))
                .count(),
            1
        );
        assert_eq!(
            items
                .iter()
                .filter(|item| item.get("call_id").and_then(Value::as_str) == Some("call-old"))
                .count(),
            2
        );
        assert_eq!(
            items
                .iter()
                .filter(|item| item.get("call_id").and_then(Value::as_str) == Some("call-new"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn durable_history_does_not_duplicate_current_turn_memory_records() {
        async fn compose() -> Json<Value> {
            Json(json!({
                "thread_id": "thread-1",
                "blocks": [],
                "recent_records": [{
                    "id": "record-current-run",
                    "thread_id": "thread-1",
                    "tenant_id": "tenant-1",
                    "source_id": "task_runner",
                    "external_record_id": null,
                    "role": "system",
                    "record_type": "message",
                    "content": "backend directory was already inspected",
                    "structured_payload": null,
                    "metadata": {"conversation_turn_id": "run-1"},
                    "summary_status": "pending",
                    "summary_id": null,
                    "summarized_at": null,
                    "created_at": "2026-08-19T09:37:14Z"
                }],
                "meta": {"summary_count": 0, "recent_record_count": 1}
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind memory engine mock");
        let address = listener.local_addr().expect("memory engine mock address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                Router::new().route("/api/memory-engine/v1/context/compose", post(compose)),
            )
            .await;
        });
        let composer = MemoryContextComposer::new_direct(
            format!("http://{address}"),
            Duration::from_secs(1),
            "task_runner",
        )
        .expect("memory composer");
        let scope = MemoryScope::thread("tenant-1", "task_runner", "thread-1");
        let durable = vec![
            json!({"role":"user","content":"build the backend"}),
            json!({"type":"reasoning","id":"rs-1","summary":[]}),
            json!({"type":"function_call","id":"fc-1","call_id":"call-1","name":"list_dir","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"call-1","output":"frontend backend"}),
        ];

        let input = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[],
            durable.as_slice(),
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("contextual input");
        server.abort();

        assert_eq!(input, Value::Array(durable));
    }

    #[tokio::test]
    async fn chat_style_durable_history_does_not_duplicate_memory_records() {
        async fn compose() -> Json<Value> {
            Json(json!({
                "thread_id": "thread-1",
                "blocks": [],
                "recent_records": [{
                    "id": "record-current-run",
                    "thread_id": "thread-1",
                    "tenant_id": "tenant-1",
                    "source_id": "task_runner",
                    "external_record_id": null,
                    "role": "system",
                    "record_type": "message",
                    "content": "backend file list was already read",
                    "structured_payload": null,
                    "metadata": {"conversation_turn_id": "run-1"},
                    "summary_status": "pending",
                    "summary_id": null,
                    "summarized_at": null,
                    "created_at": "2026-08-19T09:37:14Z"
                }],
                "meta": {"summary_count": 0, "recent_record_count": 1}
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind memory engine mock");
        let address = listener.local_addr().expect("memory engine mock address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                Router::new().route("/api/memory-engine/v1/context/compose", post(compose)),
            )
            .await;
        });
        let composer = MemoryContextComposer::new_direct(
            format!("http://{address}"),
            Duration::from_secs(1),
            "task_runner",
        )
        .expect("memory composer");
        let scope = MemoryScope::thread("tenant-1", "task_runner", "thread-1");
        let durable = vec![
            json!({"role":"user","content":"build the backend"}),
            json!({
                "role":"assistant",
                "content":"",
                "tool_calls":[{"id":"call-old","type":"function","function":{"name":"read_file","arguments":"{}"}}]
            }),
            json!({"role":"tool","tool_call_id":"call-old","content":"old huge file output"}),
            json!({
                "role":"assistant",
                "content":"",
                "tool_calls":[{"id":"call-1","type":"function","function":{"name":"list_dir","arguments":"{}"}}]
            }),
            json!({"role":"tool","tool_call_id":"call-1","content":"frontend backend"}),
        ];

        let input = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[],
            durable.as_slice(),
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("contextual input");
        server.abort();

        assert_eq!(input, Value::Array(durable));
    }

    #[tokio::test]
    async fn plain_current_input_excludes_current_turn_memory_records() {
        async fn compose() -> Json<Value> {
            Json(json!({
                "thread_id": "thread-1",
                "blocks": [],
                "recent_records": [{
                    "id": "record-current-run",
                    "thread_id": "thread-1",
                    "tenant_id": "tenant-1",
                    "source_id": "task_runner",
                    "external_record_id": null,
                    "role": "system",
                    "record_type": "message",
                    "content": "current user prompt already persisted",
                    "structured_payload": null,
                    "metadata": {"conversation_turn_id": "run-1"},
                    "summary_status": "pending",
                    "summary_id": null,
                    "summarized_at": null,
                    "created_at": "2026-08-19T09:37:14Z"
                }],
                "meta": {"summary_count": 0, "recent_record_count": 1}
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind memory engine mock");
        let address = listener.local_addr().expect("memory engine mock address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                Router::new().route("/api/memory-engine/v1/context/compose", post(compose)),
            )
            .await;
        });
        let composer = MemoryContextComposer::new_direct(
            format!("http://{address}"),
            Duration::from_secs(1),
            "task_runner",
        )
        .expect("memory composer");
        let scope = MemoryScope::thread("tenant-1", "task_runner", "thread-1");

        let input = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[],
            &[user_text_item("build the backend")],
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("contextual input");
        server.abort();

        assert!(!input
            .to_string()
            .contains("current user prompt already persisted"));
        assert!(input.to_string().contains("build the backend"));
    }

    #[tokio::test]
    async fn every_model_input_composition_fetches_latest_memory_engine_context() {
        async fn compose(State(calls): State<Arc<AtomicUsize>>) -> Json<Value> {
            let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
            Json(json!({
                "thread_id": "thread-1",
                "blocks": [{"block_type": "memory", "text": format!("memory-{call}")}],
                "recent_records": [],
                "meta": {"summary_count": 1, "recent_record_count": 0}
            }))
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind memory engine mock");
        let address = listener.local_addr().expect("memory engine mock address");
        let server_calls = Arc::clone(&calls);
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                Router::new()
                    .route("/api/memory-engine/v1/context/compose", post(compose))
                    .with_state(server_calls),
            )
            .await;
        });
        let composer = MemoryContextComposer::new_direct(
            format!("http://{address}"),
            Duration::from_secs(1),
            "task_runner",
        )
        .expect("memory composer");
        let scope = MemoryScope::thread("tenant-1", "task_runner", "thread-1");

        let first = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[],
            &[user_text_item("first")],
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("first model input");
        let second = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[],
            &[user_text_item("second")],
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("second model input");
        server.abort();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(first.to_string().contains("memory-1"));
        assert!(second.to_string().contains("memory-2"));
    }

    #[test]
    fn input_value_to_items_wraps_text_as_user_message() {
        let items = input_value_to_items(json!("hello"));
        assert_eq!(items, vec![user_text_item("hello")]);
    }

    #[test]
    fn contextual_turn_request_builds_from_model_config_and_user_text() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let runtime_options =
            AiRuntimeOptions::for_conversation("task_1").with_conversation_turn_id("run_1");

        let request =
            ContextualTurnRequest::for_user_text(&config, runtime_options, "run this task")
                .with_user_record(Some(
                    SaveRecordInput::user_message("task_1", "run this task")
                        .with_conversation_turn_id("run_1"),
                ));

        assert_eq!(request.model_request.model, "gpt-test");
        assert_eq!(
            request.runtime_options.conversation_id.as_deref(),
            Some("task_1")
        );
        assert_eq!(
            request.runtime_options.conversation_turn_id.as_deref(),
            Some("run_1")
        );
        assert_eq!(
            request.current_input_items,
            vec![user_text_item("run this task")]
        );
        assert!(request.user_record.is_some());
    }

    #[tokio::test]
    async fn contextual_turn_runner_report_captures_aborted_runtime() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:1/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let runtime_options = AiRuntimeOptions::for_conversation("task_1")
            .with_abort_checker(Some(std::sync::Arc::new(|_| true)));
        let request =
            ContextualTurnRequest::for_user_text(&config, runtime_options, "run this task");
        let runner = super::ContextualTurnRunner::new(AiRuntime::new(None), None);

        let report = runner.run_turn_report(request).await;

        assert_eq!(report.status, AiTurnStatus::Aborted);
        assert_eq!(report.error.as_deref(), Some("aborted"));
    }

    #[test]
    fn agent_turns_force_responses_for_every_gateway() {
        let mut openai = ModelRequest::openai_compatible(
            "https://api.openai.com/v1",
            "secret",
            "gpt-test",
            "openai",
            Value::Null,
        );
        let mut compatible = ModelRequest::openai_compatible(
            "https://gateway.example.test/v1",
            "secret",
            "gpt-test",
            "openai_compatible",
            Value::Null,
        );

        enable_openai_responses_protocol(&mut openai);
        enable_openai_responses_protocol(&mut compatible);

        assert!(openai.supports_responses);
        assert!(compatible.supports_responses);
    }

    #[test]
    fn runtime_turn_spec_roundtrips_and_builds_contextual_request() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        )
        .with_responses_support(true);
        let spec = RuntimeTurnSpec::for_user_text(config, "task_1", "run this task")
            .with_conversation_turn_id("run_1")
            .with_caller_model("gpt-test")
            .with_record_options(RuntimeRecordOptions::persist_all())
            .with_memory_scope(Some(
                MemoryScope::thread("tenant_1", "task_runner", "task_1")
                    .with_subject_id("contact_1"),
            ))
            .with_prefixed_input_items(vec![json!({"role":"system","content":"prefix"})])
            .with_user_record(Some(
                SaveRecordInput::user_message("task_1", "run this task")
                    .with_conversation_turn_id("run_1"),
            ))
            .with_tools(vec![json!({"type":"function","name":"tool_1"})]);

        let encoded = serde_json::to_string(&spec).expect("serialize spec");
        let decoded: RuntimeTurnSpec =
            serde_json::from_str(encoded.as_str()).expect("deserialize spec");
        let request = decoded.into_contextual_turn_request();

        assert_eq!(request.model_request.model, "gpt-test");
        assert!(request.model_request.supports_responses);
        assert_eq!(request.model_request.tools.len(), 1);
        assert_eq!(
            request.runtime_options.conversation_id.as_deref(),
            Some("task_1")
        );
        assert_eq!(
            request.runtime_options.conversation_turn_id.as_deref(),
            Some("run_1")
        );
        assert!(
            request
                .runtime_options
                .record_options
                .persist_assistant_records
        );
        assert_eq!(
            request
                .memory_scope
                .as_ref()
                .and_then(|scope| scope.subject_id.as_deref()),
            Some("contact_1")
        );
        assert_eq!(
            request.prefixed_input_items[0]["content"].as_str(),
            Some("prefix")
        );
        assert_eq!(
            request.current_input_items,
            vec![user_text_item("run this task")]
        );
        assert!(request.user_record.is_some());
    }
}
