// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{normalize_history_limit, normalize_history_offset};
use crate::api::local_connectors::{local_connector_root_path, parse_local_connector_root_path};

#[test]
fn history_limit_defaults_and_clamps() {
    assert_eq!(normalize_history_limit(None), 1200);
    assert_eq!(normalize_history_limit(Some(0)), 1);
    assert_eq!(normalize_history_limit(Some(999_999)), 5000);
}

#[test]
fn history_offset_defaults_and_is_non_negative() {
    assert_eq!(normalize_history_offset(None), 0);
    assert_eq!(normalize_history_offset(Some(-10)), 0);
    assert_eq!(normalize_history_offset(Some(25)), 25);
}

#[test]
fn terminal_execution_roots_are_local_connector_only() {
    let local_root = local_connector_root_path("device-1", "workspace-1", Some("project"));

    assert!(parse_local_connector_root_path(local_root.as_str()).is_some());
    assert!(parse_local_connector_root_path("/tmp/chatos-host-project").is_none());
    assert!(parse_local_connector_root_path("C:\\chatos-host-project").is_none());
}
