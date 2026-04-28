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

pub(crate) use self::lifecycle::{build_builtin_tool_state, build_tool_state};
pub(crate) use self::executor::McpExecutorCore;
pub(crate) use self::execution::{
    execute_tools_stream_with_registry, parse_tool_args, response_tool_name, tool_call_name,
};
pub(crate) use self::parallelism::should_parallelize_tool_batch;
pub(crate) use self::registration::{
    codex_gateway_request_tools, register_tools_from_builtin, register_tools_from_http,
    register_tools_from_stdio,
};
pub(crate) use self::state::McpToolState;

#[cfg(test)]
use self::parallelism::{
    has_conflicting_tool_profiles, paths_overlap, ToolAccessKind, ToolAccessProfile, ToolScope,
};

#[cfg(test)]
mod tests {
    use super::{
        has_conflicting_tool_profiles, paths_overlap, ToolAccessKind, ToolAccessProfile, ToolScope,
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
}
