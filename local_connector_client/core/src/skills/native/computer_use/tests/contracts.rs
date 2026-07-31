// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

#[test]
fn tool_contract_is_read_only_and_bounded() {
    let tools = tool_definitions(false);
    assert_eq!(tools.len(), 7);
    assert_eq!(tools[0]["name"], "computer_list_windows");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(names.contains(&"computer_capture_main_display"));
    assert!(names.contains(&"computer_capture_frontmost_window"));
    assert!(names.contains(&"computer_list_displays"));
    assert!(names.contains(&"computer_capture_display"));
    assert!(names.contains(&"computer_inspect_frontmost_window"));
    assert!(names.contains(&"computer_capture_window_layout"));
    assert!(tools.iter().all(|tool| tool["description"]
        .as_str()
        .is_some_and(|description| description.contains("Read-only"))));
    assert!(
        tools
            .iter()
            .all(|tool| tool.pointer("/inputSchema/additionalProperties")
                == Some(&Value::Bool(false)))
    );
}

#[test]
fn control_tools_are_published_only_for_the_approved_plugin_path() {
    let tools = tool_definitions(true);
    assert_eq!(
        tools.len(),
        if matches!(current_platform_name(), "macos" | "windows") {
            16
        } else {
            15
        }
    );
    let find = |name: &str| {
        tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
            .expect("published Computer Use tool")
    };
    assert!(find("computer_click")["description"]
        .as_str()
        .is_some_and(|description| description.contains("explicit local user approval")));
    assert!(find("computer_drag")["description"]
        .as_str()
        .is_some_and(|description| description.contains("forces mouse-up")));
    assert!(find("computer_press_key")["description"]
        .as_str()
        .is_some_and(|description| description.contains("one-time random confirmation")));
    assert!(find("computer_scroll").is_object());
    assert!(find("computer_activate_application").is_object());
    assert!(find("computer_set_frontmost_window_bounds")["description"]
        .as_str()
        .is_some_and(|description| description.contains("partial platform failures")));
    assert!(find("computer_restore_window_layout")["description"]
        .as_str()
        .is_some_and(|description| description.contains("snapshot ID")));
    if current_platform_name() == "macos" {
        assert!(
            find("computer_set_frontmost_window_fullscreen")["description"]
                .as_str()
                .is_some_and(|description| description.contains("AXFullScreen"))
        );
    }
    if current_platform_name() == "windows" {
        assert!(
            find("computer_set_frontmost_window_maximized")["description"]
                .as_str()
                .is_some_and(|description| description.contains("not true application fullscreen"))
        );
    }
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "computer_type_text"));
}

#[test]
fn windows_contract_includes_ui_automation_and_secure_text_entry() {
    let tools = tool_definitions_for_platform(true, "windows");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(tools.len(), 16);
    assert!(names.contains(&"computer_list_windows"));
    assert!(names.contains(&"computer_capture_window_layout"));
    assert!(names.contains(&"computer_inspect_frontmost_window"));
    assert!(names.contains(&"computer_capture_main_display"));
    assert!(names.contains(&"computer_capture_frontmost_window"));
    assert!(names.contains(&"computer_list_displays"));
    assert!(names.contains(&"computer_capture_display"));
    assert!(names.contains(&"computer_click"));
    assert!(names.contains(&"computer_drag"));
    assert!(names.contains(&"computer_press_key"));
    assert!(names.contains(&"computer_scroll"));
    assert!(names.contains(&"computer_activate_application"));
    assert!(names.contains(&"computer_type_text"));
    assert!(names.contains(&"computer_set_frontmost_window_bounds"));
    assert!(names.contains(&"computer_set_frontmost_window_maximized"));
    assert!(names.contains(&"computer_restore_window_layout"));
    assert!(!names.contains(&"computer_set_frontmost_window_fullscreen"));
    assert!(tools.iter().any(|tool| {
        tool["name"] == "computer_type_text"
            && tool["description"]
                .as_str()
                .is_some_and(|description| description.contains("fails closed"))
    }));
    let source = include_str!("../windows.rs");
    assert!(source.contains("struct MouseButtonReleaseGuard"));
    assert!(source.contains("MouseButtonReleaseGuard::new(up)"));
    assert!(source.contains("MouseButtonReleaseGuard::new(MOUSEEVENTF_LEFTUP)"));
    assert!(source.contains("send_mouse_flags(self.release_flags, 0)"));
    assert!(source.contains("IUIAutomationTextEditPattern"));
    assert!(source.contains("UIA_TextEditPatternId"));
    assert!(source.contains("UIA_DocumentControlTypeId"));
    assert!(source.contains("WindowsTextTargetClass::ContentEditable"));
    assert!(source.contains("pub(super) fn capture_frontmost_window()"));
    assert!(source.contains("same_identity_and_geometry"));
    assert!(source.contains("intersect_rect(window_rect, virtual_desktop_rect()?"));
    assert!(source.contains("SetWindowPos("));
    assert!(source.contains("IsZoomed(hwnd)"));
    assert!(source.contains("restore_window_bounds"));
    assert!(source.contains("restore_window_maximized_state"));
    assert!(source.contains("pub(super) fn capture_window_layout"));
    assert!(source.contains("pub(super) fn restore_window_layout"));
    assert!(source.contains("rollback_layout_windows"));
    assert!(source.contains("WS_EX_TOOLWINDOW"));
    assert!(source.contains("GW_OWNER"));
    assert!(source.contains("WS_CAPTION"));
}

#[test]
fn macos_window_control_contract_uses_native_ax_state_without_shortcuts() {
    let tools = tool_definitions_for_platform(true, "macos");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(tools.len(), 16);
    assert!(names.contains(&"computer_capture_window_layout"));
    assert!(names.contains(&"computer_restore_window_layout"));
    assert!(names.contains(&"computer_set_frontmost_window_bounds"));
    assert!(names.contains(&"computer_set_frontmost_window_fullscreen"));
    assert!(!names.contains(&"computer_set_frontmost_window_maximized"));
    assert!(FRONTMOST_WINDOW_CONTROL_TARGET_JXA.contains("AXPosition"));
    assert!(FRONTMOST_WINDOW_CONTROL_TARGET_JXA.contains("AXSize"));
    assert!(FRONTMOST_WINDOW_CONTROL_TARGET_JXA.contains("AXFullScreen"));
    assert!(SET_FRONTMOST_WINDOW_BOUNDS_JXA.contains("matchesApproved(before, approved)"));
    assert!(SET_FRONTMOST_WINDOW_BOUNDS_JXA.contains("recoveryResult(approved)"));
    assert!(
        SET_FRONTMOST_WINDOW_FULLSCREEN_JXA.contains("fullscreen_attribute.value.set(requested)")
    );
    assert!(RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA
        .contains("fullscreen_attribute.value.set(approved.fullscreen)"));
    assert!(CAPTURE_WINDOW_LAYOUT_JXA.contains("AXStandardWindow"));
    assert!(PREFLIGHT_WINDOW_LAYOUT_JXA.contains("process_identity"));
    assert!(RESTORE_WINDOW_LAYOUT_JXA.contains("rollback(systemEvents"));
    assert!(RESTORE_WINDOW_LAYOUT_JXA.contains("!recovery.complete"));
    assert!(ROLLBACK_WINDOW_LAYOUT_JXA.contains("snapshot.windows.length - 1"));
    assert!(!SET_FRONTMOST_WINDOW_FULLSCREEN_JXA.contains("keystroke"));
}

#[test]
fn window_bounds_and_approval_guard_are_bounded_and_fail_closed() {
    let request = parse_window_bounds_request(&json!({
        "x": -1200,
        "y": 80,
        "width": 1280,
        "height": 720,
    }))
    .unwrap();
    assert_eq!(request.geometry(), "1280 x 720 @ -1200, 80");
    assert!(parse_window_bounds_request(&json!({
        "x": 0,
        "y": 0,
        "width": 63,
        "height": 720,
    }))
    .is_err());
    assert!(parse_window_bounds_request(&json!({
        "x": 0,
        "y": 0,
        "width": 1280,
        "height": 720,
        "pid": 42,
    }))
    .is_err());
    assert!(parse_window_fullscreen_request(&json!({"fullscreen": true})).unwrap());
    assert!(parse_window_fullscreen_request(&json!({"fullscreen": 1})).is_err());
    assert!(!parse_window_maximized_request(&json!({"maximized": false})).unwrap());

    let guard = ApprovedFrontmostWindowGuard {
        platform: "macos".to_string(),
        application: "Example".to_string(),
        pid: 42,
        window_id: "1001".to_string(),
        position: [10.0, 20.0],
        size: [1280.0, 720.0],
        fullscreen: Some(false),
        maximized: None,
        position_settable: true,
        size_settable: true,
        fullscreen_settable: true,
    };
    let encoded = vec![window_approval_argument(&guard).unwrap()];
    assert!(!encoded.join(" ").contains("title"));
    assert_eq!(approved_window_guard(Some(&encoded)).unwrap(), guard);
    assert!(approved_window_guard(Some(&[])).is_err());
    let display_layout = vec![ApprovedDisplayGuard {
        index: 1,
        display_id: 99,
        is_main: true,
        origin_x: 0.0,
        origin_y: 0.0,
        width: 1920.0,
        height: 1080.0,
        pixels_wide: 3840,
        pixels_high: 2160,
        rotation_degrees: 0.0,
    }];
    validate_requested_window_bounds_against_layout(&request, &display_layout).unwrap();
    let encoded_layout = vec![window_display_layout_approval_argument(&display_layout).unwrap()];
    assert_eq!(
        display::approved_window_display_layout(Some(&encoded_layout)).unwrap(),
        display_layout
    );
    assert!(display::approved_window_display_layout(Some(&[])).is_err());
    let mut invalid = guard.clone();
    invalid.fullscreen = None;
    assert!(invalid.validate().is_err());
}

#[test]
fn window_layout_snapshot_is_opaque_bounded_one_shot_and_redacted() {
    let display_layout = vec![ApprovedDisplayGuard {
        index: 1,
        display_id: 99,
        is_main: true,
        origin_x: 0.0,
        origin_y: 0.0,
        width: 1920.0,
        height: 1080.0,
        pixels_wide: 3840,
        pixels_high: 2160,
        rotation_degrees: 0.0,
    }];
    let window = ApprovedWindowLayoutGuard {
        platform: current_platform_name().to_string(),
        application: "Example".to_string(),
        process_identity: "bundle:com.example.app".to_string(),
        pid: 42,
        window_id: "1001".to_string(),
        position: [100.0, 120.0],
        size: [1280.0, 720.0],
    };
    let mut snapshot = WindowLayoutSnapshot {
        schema_version: WINDOW_LAYOUT_SCHEMA_VERSION,
        snapshot_id: uuid::Uuid::new_v4().hyphenated().to_string(),
        snapshot_sha256: String::new(),
        platform: current_platform_name().to_string(),
        display_layout,
        windows: vec![window],
        excluded_window_count: 2,
        truncated: false,
    };
    snapshot.snapshot_sha256 = window_layout_sha256(&snapshot).unwrap();
    snapshot.validate().unwrap();

    let reference_arguments = json!({
        "snapshot_id": snapshot.snapshot_id,
        "snapshot_sha256": snapshot.snapshot_sha256,
    });
    let reference = parse_window_layout_reference(&reference_arguments).unwrap();
    assert!(parse_window_layout_reference(&json!({
        "snapshot_id": reference.snapshot_id,
        "snapshot_sha256": reference.snapshot_sha256,
        "pid": 42,
    }))
    .is_err());

    let approved_argument = window_layout_approval_argument(&snapshot).unwrap();
    let approved_arguments = vec![approved_argument];
    assert_eq!(
        approved_window_layout_snapshot(Some(approved_arguments.as_slice())).unwrap(),
        snapshot
    );
    assert!(redact_approval_arguments("computer_restore_window_layout"));

    store_window_layout_snapshot(snapshot.clone()).unwrap();
    assert_eq!(stored_window_layout_snapshot(&reference).unwrap(), snapshot);
    consume_approved_window_layout_snapshot(
        &reference_arguments,
        Some(approved_arguments.as_slice()),
    )
    .unwrap();
    assert!(stored_window_layout_snapshot(&reference).is_err());

    let public = finalize_window_layout_capture(
        serde_json::to_value(WindowLayoutCapturePayload {
            platform: snapshot.platform.clone(),
            display_layout: snapshot.display_layout.clone(),
            windows: snapshot.windows.clone(),
            excluded_window_count: 0,
            truncated: false,
        })
        .unwrap(),
    )
    .unwrap();
    let serialized = public.to_string();
    assert!(!serialized.contains("com.example.app"));
    assert!(!serialized.contains("1001"));
    assert_eq!(
        public.pointer("/_structured_result/persisted"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        public.pointer("/_structured_result/model_supplied_window_identities_or_coordinates"),
        Some(&Value::Bool(false))
    );

    let now = Instant::now();
    let mut at_capacity = BTreeMap::new();
    for _ in 0..MAX_WINDOW_LAYOUT_SNAPSHOTS {
        let mut stored_snapshot = snapshot.clone();
        stored_snapshot.snapshot_id = uuid::Uuid::new_v4().hyphenated().to_string();
        stored_snapshot.snapshot_sha256 = window_layout_sha256(&stored_snapshot).unwrap();
        at_capacity.insert(
            stored_snapshot.snapshot_id.clone(),
            StoredWindowLayoutSnapshot {
                captured_at: now,
                snapshot: stored_snapshot,
            },
        );
    }
    prune_expired_window_layout_snapshots(&mut at_capacity, now);
    assert_eq!(at_capacity.len(), MAX_WINDOW_LAYOUT_SNAPSHOTS);
    evict_window_layout_snapshot_for_insert(&mut at_capacity);
    assert_eq!(at_capacity.len(), MAX_WINDOW_LAYOUT_SNAPSHOTS - 1);
}
