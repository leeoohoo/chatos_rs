#[path = "mcp_execution_core/execution.rs"]
mod execution;
#[path = "mcp_execution_core/executor.rs"]
mod executor;
#[path = "mcp_execution_core/lifecycle.rs"]
mod lifecycle;
#[path = "mcp_execution_core/parallelism.rs"]
mod parallelism;
#[path = "mcp_execution_core/registration.rs"]
mod registration;
#[path = "mcp_execution_core/state.rs"]
mod state;

pub(crate) use self::execution::{
    execute_tools_stream_with_registry, parse_tool_args, response_tool_name, tool_call_name,
};
pub(crate) use self::executor::McpExecutorCore;
pub(crate) use self::lifecycle::{build_builtin_tool_state, build_tool_state};
#[cfg(test)]
pub(crate) use self::parallelism::should_parallelize_tool_batch;
pub(crate) use self::registration::{
    codex_gateway_request_tools, register_tools_from_builtin, register_tools_from_http,
    register_tools_from_stdio,
};
pub(crate) use self::state::McpToolState;

#[cfg(test)]
use self::execution::execute_tools_stream_parallel;
#[cfg(test)]
use self::parallelism::{
    has_conflicting_tool_profiles, paths_overlap, ToolAccessKind, ToolAccessProfile, ToolScope,
};

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use serde_json::{json, Value};

    use super::{
        execute_tools_stream_parallel, has_conflicting_tool_profiles, paths_overlap,
        ToolAccessKind, ToolAccessProfile, ToolScope,
    };

    #[test]
    fn conflict_policy_detects_overlapping_write_paths() {
        let profiles = vec![
            ToolAccessProfile {
                kind: ToolAccessKind::Read,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "src/services".to_string(),
                },
            },
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "src".to_string(),
                },
            },
        ];
        assert!(has_conflicting_tool_profiles(profiles.as_slice()));
    }

    #[test]
    fn conflict_policy_allows_disjoint_write_and_read_paths() {
        let profiles = vec![
            ToolAccessProfile {
                kind: ToolAccessKind::Read,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "docs".to_string(),
                },
            },
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "local".to_string(),
                    path: "src".to_string(),
                },
            },
        ];
        assert!(!has_conflicting_tool_profiles(profiles.as_slice()));
    }

    #[test]
    fn conflict_policy_allows_same_path_when_locator_is_different() {
        let profiles = vec![
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "remote:server_a".to_string(),
                    path: "srv/config.toml".to_string(),
                },
            },
            ToolAccessProfile {
                kind: ToolAccessKind::Write,
                scope: ToolScope::Path {
                    locator: "remote:server_b".to_string(),
                    path: "srv/config.toml".to_string(),
                },
            },
        ];
        assert!(!has_conflicting_tool_profiles(profiles.as_slice()));
    }

    #[test]
    fn path_overlap_treats_root_as_overlapping_everything() {
        assert!(paths_overlap(".", "src"));
    }

    #[tokio::test]
    async fn parallel_tool_execution_returns_error_result_when_task_panics() {
        let tool_calls = vec![
            json!({
                "id": "call_ok",
                "type": "function",
                "function": {
                    "name": "list_dir",
                    "arguments": "{}"
                }
            }),
            json!({
                "id": "call_panic",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{}"
                }
            }),
        ];
        let panic_once = Arc::new(AtomicUsize::new(0));

        let results = execute_tools_stream_parallel(
            &tool_calls,
            Some("session-1"),
            Some("turn-1"),
            None,
            None,
            None,
            move |tool_name: String,
                  _args: Value,
                  _session_id: Option<String>,
                  _turn_id: Option<String>,
                  _caller_model: Option<String>,
                  _caller_model_runtime,
                  _on_stream_chunk| {
                let panic_once = Arc::clone(&panic_once);
                async move {
                    if tool_name == "search_files" && panic_once.fetch_add(1, Ordering::SeqCst) == 0
                    {
                        panic!("simulated utf8 boundary panic");
                    }
                    Ok((format!("ok:{tool_name}"), None))
                }
            },
        )
        .await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_call_id, "call_ok");
        assert!(results[0].success);
        assert_eq!(results[1].tool_call_id, "call_panic");
        assert!(!results[1].success);
        assert!(results[1].is_error);
        assert!(results[1].content.contains("internal panic"));
    }
}
