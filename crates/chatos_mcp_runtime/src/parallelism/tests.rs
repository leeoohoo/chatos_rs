// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::json;

use super::{
    has_conflicting_tool_profiles, normalize_scope_locator, normalize_scope_path, paths_overlap,
    should_parallelize_tool_batch, ToolAccessKind, ToolAccessProfile, ToolParallelismInfo,
    ToolScope,
};

struct TestToolInfo {
    original_name: String,
    server_name: String,
}

impl ToolParallelismInfo for TestToolInfo {
    fn original_name(&self) -> &str {
        self.original_name.as_str()
    }

    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }
}

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
fn conflict_policy_allows_disjoint_or_different_locator_paths() {
    let disjoint = vec![
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
    assert!(!has_conflicting_tool_profiles(disjoint.as_slice()));

    let different_locators = vec![
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
    assert!(!has_conflicting_tool_profiles(
        different_locators.as_slice()
    ));
}

#[test]
fn scope_normalization_is_conservative() {
    assert_eq!(
        normalize_scope_path("src/../config/app.toml"),
        "config/app.toml"
    );
    assert_eq!(normalize_scope_path("./"), ".");
    assert_eq!(
        normalize_scope_locator(" Remote:SERVER-A "),
        "remote:server-a"
    );
    assert!(paths_overlap(".", "src"));
}

#[test]
fn safe_read_batch_uses_generic_metadata_adapter() {
    let metadata = HashMap::from([
        (
            "local_read_file_raw".to_string(),
            TestToolInfo {
                original_name: "read_file_raw".to_string(),
                server_name: "local".to_string(),
            },
        ),
        (
            "local_list_dir".to_string(),
            TestToolInfo {
                original_name: "list_dir".to_string(),
                server_name: "local".to_string(),
            },
        ),
    ]);
    let calls = vec![
        json!({"function": {"name": "local_read_file_raw", "arguments": "{\"path\":\"src/lib.rs\"}"}}),
        json!({"function": {"name": "local_list_dir", "arguments": "{\"path\":\"src\"}"}}),
    ];

    assert!(should_parallelize_tool_batch(calls.as_slice(), &metadata));
}

#[test]
fn malformed_or_unknown_tool_batch_stays_sequential() {
    let metadata = HashMap::from([(
        "local_list_dir".to_string(),
        TestToolInfo {
            original_name: "list_dir".to_string(),
            server_name: "local".to_string(),
        },
    )]);
    let malformed = vec![
        json!({"function": {"name": "local_list_dir", "arguments": "{"}}),
        json!({"function": {"name": "local_list_dir", "arguments": "{}"}}),
    ];
    assert!(!should_parallelize_tool_batch(
        malformed.as_slice(),
        &metadata
    ));
}
