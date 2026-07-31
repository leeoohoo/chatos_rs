// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;
#[cfg(target_os = "macos")]
use std::os::unix::process::ExitStatusExt;

#[cfg(target_os = "macos")]
#[test]
fn decoded_observation_is_marked_read_only() {
    let status = ExitStatus::from_raw(0);
    let value = decode_jxa_result(status, br#"{"platform":"macos"}"#, b"").unwrap();
    assert_eq!(value["success"], true);
    assert_eq!(value["mode"], "read_only");
    assert_eq!(value["sensitive_text_policy"], "editable_values_redacted");
}

#[cfg(target_os = "macos")]
#[test]
fn permission_failures_are_reported_without_raw_automation_noise() {
    let status = ExitStatus::from_raw(1 << 8);
    let error = classify_macos_observer_error(
        "System Events got an error: osascript is not allowed assistive access. (-1719)",
        status,
    );
    assert_eq!(
        error.to_string(),
        "macOS Accessibility permission is required for Computer Use observation"
    );
}

#[test]
fn editable_values_are_redacted_by_the_embedded_inspection_script() {
    assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("role === \"AXTextField\""));
    assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("AXIsEditable"));
    assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("AXEditableAncestor"));
    assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("if (editable)"));
    assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("node.value_redacted = true"));
    assert!(INSPECT_FRONTMOST_WINDOW_JXA
        .find("if (editable)")
        .is_some_and(|editable| INSPECT_FRONTMOST_WINDOW_JXA
            .find("node.value = text")
            .is_some_and(|value| editable < value)));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_text_target_classifier_requires_explicit_writability() {
    assert_eq!(
        classify_macos_text_target(true, false, false, true, false).unwrap(),
        MacTextTargetClass::NativeTextControl
    );
    assert_eq!(
        classify_macos_text_target(false, true, true, false, true).unwrap(),
        MacTextTargetClass::ContentEditable
    );
    assert!(classify_macos_text_target(true, false, false, false, false).is_err());
    assert!(classify_macos_text_target(false, true, false, false, true).is_err());
    assert!(classify_macos_text_target(false, true, true, false, false).is_err());
    assert!(classify_macos_text_target(false, false, true, true, true).is_err());
}

#[test]
fn screenshot_payload_separates_transient_image_from_persistable_metadata() {
    let display = DisplayTarget {
        index: 2,
        display_id: 99,
        is_main: false,
        origin_x: 1440.0,
        origin_y: 0.0,
        width: 1920.0,
        height: 1080.0,
        pixels_wide: 3840,
        pixels_high: 2160,
        rotation_degrees: 0.0,
    };
    let result = screenshot_result(&[0xff, 0xd8, 0xff, 0x00, 0x01], &display).unwrap();
    assert!(result
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/jpeg;base64,")));
    let structured = result
        .get("_structured_result")
        .expect("structured metadata");
    assert_eq!(structured["persisted"], false);
    assert_eq!(structured["display_index"], 2);
    assert_eq!(structured["capture_scope"], "selected_display");
    assert!(!structured.to_string().contains("base64"));
}

#[test]
fn frontmost_window_screenshot_is_transient_and_geometry_bound() {
    let target = FrontmostWindowCaptureTarget {
        platform: "windows",
        application: "Example.exe".to_string(),
        pid: 42,
        window_id: "0x1234".to_string(),
        title: "Example".to_string(),
        position: [-20.0, 10.0],
        size: [1024.0, 768.0],
        capture_position: [0.0, 10.0],
        capture_size: [1004.0, 768.0],
        clipped_to_visible_desktop: true,
    };
    let result =
        frontmost_window_screenshot_result(b"\x89PNG\r\n\x1a\nwindow-pixels", &target).unwrap();
    assert!(result
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    let structured = &result["_structured_result"];
    assert_eq!(structured["capture_scope"], "frontmost_window");
    assert_eq!(structured["window_id"], "0x1234");
    assert_eq!(structured["window_position"], json!([-20.0, 10.0]));
    assert_eq!(structured["capture_position"], json!([0.0, 10.0]));
    assert_eq!(structured["clipped_to_visible_desktop"], true);
    assert_eq!(
        structured["identity_and_geometry_revalidated_after_capture"],
        true
    );
    assert_eq!(structured["persisted"], false);
    assert!(!structured.to_string().contains("base64"));
}

#[test]
fn post_action_observation_attaches_transient_pixels_without_persisting_them() {
    let display = DisplayTarget {
        index: 2,
        display_id: 99,
        is_main: false,
        origin_x: 1440.0,
        origin_y: 0.0,
        width: 1920.0,
        height: 1080.0,
        pixels_wide: 3840,
        pixels_high: 2160,
        rotation_degrees: 0.0,
    };
    let screenshot = screenshot_result(&[0xff, 0xd8, 0xff, 0x00, 0x01], &display).unwrap();
    let target = PostActionObservationTarget::ApprovedDisplay(ApprovedDisplayGuard::from(&display));
    let result = build_post_action_result(
        "computer_click",
        json!({"success": true, "action": "click"}),
        &target,
        Ok(screenshot),
    );
    assert!(result
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/jpeg;base64,")));
    let structured = result
        .get("_structured_result")
        .expect("post-action structured result");
    assert_eq!(structured["success"], true);
    assert_eq!(structured["post_action_observation"]["captured"], true);
    assert_eq!(structured["post_action_observation"]["persisted"], false);
    assert_eq!(structured["recovery"]["automatic_replay_safe"], false);
    assert!(!structured.to_string().contains("base64"));
}

#[test]
fn window_post_action_observation_binds_exact_window_and_requested_state() {
    let approved = ApprovedFrontmostWindowGuard {
        platform: "windows".to_string(),
        application: "Example.exe".to_string(),
        pid: 42,
        window_id: "0x1234".to_string(),
        position: [10.0, 20.0],
        size: [800.0, 600.0],
        fullscreen: None,
        maximized: Some(false),
        position_settable: true,
        size_settable: true,
        fullscreen_settable: false,
    };
    let guard = WindowControlRollbackGuard::Bounds {
        request: WindowBoundsRequest {
            x: 120,
            y: 80,
            width: 1000,
            height: 700,
        },
        approved,
    };
    let current = ApprovedFrontmostWindowGuard {
        position: [120.0, 80.0],
        size: [1000.0, 700.0],
        ..match &guard {
            WindowControlRollbackGuard::Bounds { approved, .. } => approved.clone(),
            _ => unreachable!(),
        }
    };
    assert!(guard.matches_applied_state(&current));

    let screenshot_target = FrontmostWindowCaptureTarget {
        platform: "windows",
        application: "Example.exe".to_string(),
        pid: 42,
        window_id: "0x1234".to_string(),
        title: "A dynamic title is not identity".to_string(),
        position: [120.0, 80.0],
        size: [1000.0, 700.0],
        capture_position: [120.0, 80.0],
        capture_size: [1000.0, 700.0],
        clipped_to_visible_desktop: false,
    };
    let screenshot = frontmost_window_screenshot_result(
        b"\x89PNG\r\n\x1a\nwindow-observation",
        &screenshot_target,
    )
    .unwrap();
    let target = guard.observation_target();
    let result = build_post_action_result(
        "computer_set_frontmost_window_bounds",
        json!({"success": true, "target_geometry_applied": true}),
        &target,
        Ok(screenshot),
    );
    assert_eq!(
        result["_structured_result"]["post_action_observation"]["target"]["scope"],
        "frontmost_window"
    );
    assert_eq!(
        result["_structured_result"]["post_action_observation"]["captured"],
        true
    );

    let restored = match &guard {
        WindowControlRollbackGuard::Bounds { approved, .. } => approved.clone(),
        _ => unreachable!(),
    };
    assert!(guard.matches_target_identity(&restored));
    assert!(!guard.matches_applied_state(&restored));

    let changed = ApprovedFrontmostWindowGuard {
        window_id: "0x9999".to_string(),
        ..current
    };
    assert!(!guard.matches_applied_state(&changed));
}

#[test]
fn post_action_observation_failure_never_marks_an_executed_action_for_replay() {
    let target = PostActionObservationTarget::MainDisplay;
    let result = build_post_action_result(
        "computer_type_text",
        json!({
            "success": true,
            "action": "type_text",
            "character_count": 8,
            "sha256": "redacted-hash"
        }),
        &target,
        Err("capture_timeout"),
    );
    assert!(result.get("_model_input").is_none());
    let structured = result
        .get("_structured_result")
        .expect("post-action failure metadata");
    assert_eq!(structured["success"], true);
    assert_eq!(structured["post_action_observation"]["captured"], false);
    assert_eq!(
        structured["post_action_observation"]["reason"],
        "capture_timeout"
    );
    assert_eq!(structured["recovery"]["action_already_executed"], true);
    assert_eq!(structured["recovery"]["automatic_replay_safe"], false);
}

#[cfg(target_os = "macos")]
#[test]
fn screenshot_permission_failures_hide_raw_capture_noise() {
    let status = ExitStatus::from_raw(1 << 8);
    let error = classify_macos_screenshot_error(
        "screencapture: could not create image from display 1",
        status,
    );
    assert_eq!(
        error.to_string(),
        "macOS Screen Recording permission is required for Computer Use screenshots"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn embedded_jxa_observers_compile_without_requesting_permissions() {
    let temp = tempfile::tempdir().expect("temporary script directory");
    for (index, script) in [
        LIST_WINDOWS_JXA,
        CAPTURE_WINDOW_LAYOUT_JXA,
        PREFLIGHT_WINDOW_LAYOUT_JXA,
        RESTORE_WINDOW_LAYOUT_JXA,
        ROLLBACK_WINDOW_LAYOUT_JXA,
        INSPECT_FRONTMOST_WINDOW_JXA,
        FRONTMOST_WINDOW_CAPTURE_TARGET_JXA,
        FRONTMOST_WINDOW_CONTROL_TARGET_JXA,
        SET_FRONTMOST_WINDOW_BOUNDS_JXA,
        RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA,
        SET_FRONTMOST_WINDOW_FULLSCREEN_JXA,
        RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA,
        LOOKUP_APPLICATION_JXA,
        ACTIVATE_APPLICATION_JXA,
        FRONTMOST_APPLICATION_JXA,
        RESTORE_APPLICATION_JXA,
    ]
    .into_iter()
    .enumerate()
    {
        let output_path = temp.path().join(format!("observer-{index}.scpt"));
        let output = Command::new("/usr/bin/osacompile")
            .args(["-l", "JavaScript", "-e", script, "-o"])
            .arg(output_path.as_os_str())
            .output()
            .expect("compile embedded JXA observer");
        assert!(
            output.status.success(),
            "JXA compilation failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        );
    }
}
