// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

#[test]
fn click_contract_supports_single_right_and_approved_left_double_clicks() {
    let tools = tool_definitions(true);
    let click = tools
        .iter()
        .find(|tool| tool["name"] == "computer_click")
        .expect("click tool");
    assert_eq!(
        click.pointer("/inputSchema/properties/click_count/enum"),
        Some(&json!([1, 2]))
    );
    assert!(click["description"]
        .as_str()
        .is_some_and(|description| description.contains("double-click")));
    assert_eq!(parse_click_count(&json!({}), "left").unwrap(), 1);
    assert_eq!(
        parse_click_count(&json!({"click_count":2}), "left").unwrap(),
        2
    );
    assert_eq!(
        parse_click_count(&json!({"click_count":1}), "right").unwrap(),
        1
    );
    assert!(parse_click_count(&json!({"click_count":2}), "right").is_err());
    assert!(parse_click_count(&json!({"click_count":0}), "left").is_err());
    assert!(parse_click_count(&json!({"click_count":3}), "left").is_err());

    let display = DisplayTarget {
        index: 2,
        display_id: 42,
        is_main: false,
        origin_x: -1920.0,
        origin_y: 0.0,
        width: 1920.0,
        height: 1080.0,
        pixels_wide: 3840,
        pixels_high: 2160,
        rotation_degrees: 0.0,
    };
    let action = ClickAction {
        display,
        x: 320.0,
        y: 240.0,
        global_x: -1600.0,
        global_y: 240.0,
        button: "left",
        click_count: 2,
    };
    let approval = click_approval_arguments(&action).unwrap();
    assert!(approval.iter().any(|value| value == "--button=left"));
    assert!(approval.iter().any(|value| value == "--click-count=2"));
    let result = click_result(&action);
    assert_eq!(result["click_count"], 2);
    assert_eq!(result["interruptible_between_clicks"], true);
}

#[test]
fn drag_contract_is_bounded_cancel_aware_and_display_guarded() {
    let tools = tool_definitions(true);
    let drag = tools
        .iter()
        .find(|tool| tool["name"] == "computer_drag")
        .expect("drag tool");
    assert_eq!(
        drag.pointer("/inputSchema/properties/duration_ms/minimum")
            .and_then(Value::as_u64),
        Some(MIN_DRAG_DURATION_MS)
    );
    assert_eq!(
        drag.pointer("/inputSchema/properties/duration_ms/maximum")
            .and_then(Value::as_u64),
        Some(MAX_DRAG_DURATION_MS)
    );
    assert_eq!(drag_step_count(MIN_DRAG_DURATION_MS), 5);
    assert_eq!(drag_step_count(MAX_DRAG_DURATION_MS), MAX_DRAG_STEPS);

    let cancelled = AtomicBool::new(false);
    ensure_action_not_cancelled(Some(&cancelled)).unwrap();
    cancelled.store(true, Ordering::SeqCst);
    assert!(ensure_action_not_cancelled(Some(&cancelled)).is_err());

    let display = DisplayTarget {
        index: 2,
        display_id: 42,
        is_main: false,
        origin_x: -1920.0,
        origin_y: 0.0,
        width: 1920.0,
        height: 1080.0,
        pixels_wide: 3840,
        pixels_high: 2160,
        rotation_degrees: 0.0,
    };
    let approved = vec![display_approval_argument(&display).unwrap()];
    validate_approved_display(&display, Some(approved.as_slice())).unwrap();
    let mut drifted = display.clone();
    drifted.origin_x = 0.0;
    assert!(validate_approved_display(&drifted, Some(approved.as_slice())).is_err());
    assert!(validate_approved_display(&display, Some(&[])).is_err());
}

#[test]
fn key_action_contract_is_allowlisted_and_stable_for_approval() {
    let (command, args, audit) = approval_command(
        "computer_press_key",
        &json!({"key":"enter", "modifiers":["shift", "command"]}),
    )
    .unwrap();
    assert_eq!(command, "computer_press_key");
    assert_eq!(args, ["--key=enter", "--modifiers=command+shift"]);
    assert_eq!(audit.kind, "computer_use");
    assert_eq!(audit.operation, "computer_press_key");
    assert!(audit
        .details
        .iter()
        .any(|detail| detail.key == "modifiers" && detail.value == "command+shift"));
    assert!(audit.details.iter().any(|detail| {
        detail.key == "confirmation_risk" && detail.value == "submit_or_activate"
    }));
    let (_, _, escape_audit) = approval_command(
        "computer_press_key",
        &json!({"key":"escape", "modifiers":[]}),
    )
    .unwrap();
    assert!(!escape_audit
        .details
        .iter()
        .any(|detail| detail.key == "confirmation_risk"));
    assert!(approval_command("computer_press_key", &json!({"key":"a", "modifiers":[]})).is_err());
    assert!(approval_command(
        "computer_press_key",
        &json!({"key":"enter", "modifiers":["shift", "shift"]})
    )
    .is_err());
}

#[test]
fn typed_text_is_visible_for_approval_but_not_for_history_or_results() {
    let secret = "review this exact text";
    let (command, args, audit) =
        approval_command("computer_type_text", &json!({"text": secret})).unwrap();
    assert_eq!(command, "computer_type_text");
    assert!(args.join(" ").contains(secret));
    assert!(redact_approval_arguments("computer_type_text"));
    let serialized_audit = serde_json::to_string(&audit).unwrap();
    assert!(!serialized_audit.contains(secret));
    assert_eq!(
        audit.privacy.as_deref(),
        Some("text_redacted_from_persistent_history")
    );
    assert!(audit.details.iter().any(|detail| {
        detail.key == "text_sha256"
            && detail.value == hex::encode(Sha256::digest(secret.as_bytes()))
    }));
    assert!(audit.details.iter().any(|detail| {
        detail.key == "confirmation_risk" && detail.value == "sensitive_text_entry"
    }));

    let arguments = json!({"text": secret});
    let action = parse_typed_text(&arguments).unwrap();
    let result = typed_text_result(&action);
    assert!(!result.to_string().contains(secret));
    assert_eq!(result["character_count"], secret.chars().count());
    assert_eq!(result["text_persisted"], false);
    assert_eq!(
        result["sha256"],
        hex::encode(Sha256::digest(secret.as_bytes()))
    );
}

#[test]
fn typed_text_rejects_controls_invisible_formatting_and_oversize_input() {
    assert!(parse_typed_text(&json!({"text": "line one\nline two"})).is_err());
    assert!(parse_typed_text(&json!({"text": "safe\u{202e}spoof"})).is_err());
    assert!(parse_typed_text(&json!({"text": "x".repeat(257)})).is_err());
}

#[test]
fn scroll_contract_is_bounded_non_zero_and_stable_for_approval() {
    let (command, args, audit) =
        approval_command("computer_scroll", &json!({"delta_y": -240, "delta_x": 20})).unwrap();
    assert_eq!(command, "computer_scroll");
    assert_eq!(args, ["--delta-y=-240", "--delta-x=20"]);
    assert!(audit
        .details
        .iter()
        .any(|detail| detail.key == "delta_y" && detail.value == "-240"));
    assert!(parse_scroll(&json!({})).is_err());
    assert!(parse_scroll(&json!({"delta_y": 1201})).is_err());
    assert!(parse_scroll(&json!({"delta_y": 1.5})).is_err());
}

#[test]
fn application_activation_accepts_only_a_positive_pid_and_sanitizes_labels() {
    assert_eq!(parse_application_pid(&json!({"pid": 42})).unwrap(), 42);
    assert!(parse_application_pid(&json!({"pid": 0})).is_err());
    assert!(parse_application_pid(&json!({"pid": "42"})).is_err());
    assert_eq!(safe_approval_label("Safe\u{202e}Name"), "Safe�Name");
    assert_eq!(
        approved_application_name(Some(&["--application-json=\"Safari\"".to_string()])).unwrap(),
        "Safari"
    );
    assert!(approved_application_name(Some(&[])).is_err());
    assert!(ACTIVATE_APPLICATION_JXA.contains("argv[1]"));
    assert!(ACTIVATE_APPLICATION_JXA.contains("approved application identity changed"));
    assert!(ACTIVATE_APPLICATION_JXA.contains("frontmost application changed before activation"));
    assert!(ACTIVATE_APPLICATION_JXA.contains("frontmost.set(true)"));
    assert!(FRONTMOST_APPLICATION_JXA.contains("candidate.frontmost()"));
    assert!(RESTORE_APPLICATION_JXA.contains("target.frontmost()"));
    assert!(RESTORE_APPLICATION_JXA.contains("previous.frontmost.set(true)"));
    assert!(RESTORE_APPLICATION_JXA.contains("foreground_changed_after_activation"));
    let source = include_str!("../windows.rs");
    assert!(source.contains("ApplicationActivationRollbackGuard"));
    assert!(source.contains("foreground application changed before activation"));
    assert!(source.contains("GetForegroundWindow() != target_hwnd"));
    assert!(source.contains("SetForegroundWindow(previous_hwnd)"));
    assert!(source.contains("target_was_minimized"));
}

#[test]
fn application_activation_recovery_metadata_is_narrow_and_never_replay_safe() {
    let action = with_application_activation_recovery(
        json!({"success": true, "action": "activate_application"}),
        json!({
            "scope": "frontmost_application_activation_only",
            "rollback_on_in_flight_cancel": true,
            "attempted": true,
            "restored": true,
            "reason": "cancelled_activation_restored",
            "application_content_rollback": false,
            "window_geometry_rollback": false,
        }),
    );
    let result = build_post_action_result(
        "computer_activate_application",
        action,
        &PostActionObservationTarget::MainDisplay,
        Err("cancelled_after_action"),
    );
    let structured = &result["_structured_result"];
    assert_eq!(
        structured["application_state_recovery"]["scope"],
        "frontmost_application_activation_only"
    );
    assert_eq!(structured["application_state_recovery"]["restored"], true);
    assert_eq!(
        structured["application_state_recovery"]["application_content_rollback"],
        false
    );
    assert_eq!(structured["recovery"]["automatic_replay_safe"], false);
}

#[test]
fn bounded_integer_rejects_invalid_ranges_and_types() {
    assert_eq!(
        bounded_integer(&json!({}), "limit", 40, 1, 100).unwrap(),
        40
    );
    assert!(bounded_integer(&json!({"limit": 0}), "limit", 40, 1, 100).is_err());
    assert!(bounded_integer(&json!({"limit": "40"}), "limit", 40, 1, 100).is_err());
}
