// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::approval::{ApprovalActionAudit, ApprovalActionAuditDetail};

use super::action::{
    click_approval_arguments, key_confirmation_risk, parse_click, parse_drag, parse_key_action,
    parse_scroll, parse_typed_text,
};
use super::{
    active_display_layout_guard, display_approval_argument, frontmost_window_control_target,
    lookup_application, parse_application_pid, parse_window_bounds_request,
    parse_window_fullscreen_request, parse_window_layout_reference, parse_window_maximized_request,
    safe_approval_label, stored_window_layout_snapshot,
    validate_requested_window_bounds_against_layout, validate_window_bounds_capability,
    validate_window_fullscreen_capability, validate_window_layout_snapshot_for_approval,
    validate_window_maximized_capability, window_approval_argument,
    window_display_layout_approval_argument, window_layout_application_summary,
    window_layout_approval_argument, DisplayTarget, CONTROL_OPERATIONS,
};

pub(super) fn requires_interactive_approval(operation: &str) -> bool {
    CONTROL_OPERATIONS.contains(&operation)
}

pub(super) fn approval_command(
    operation: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>, ApprovalActionAudit)> {
    match operation {
        "computer_click" => {
            let action = parse_click(arguments)?;
            let recovery = if action.click_count == 2 {
                "post_action_observation_double_click_and_mouse_up_recovery"
            } else {
                "post_action_observation_and_mouse_up_recovery"
            };
            Ok((
                operation.to_string(),
                click_approval_arguments(&action)?,
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("display_index", action.display.index),
                        audit_detail("display_id", action.display.display_id),
                        audit_detail("point", format_point(action.x, action.y)),
                        audit_detail("button", action.button),
                        audit_detail("click_count", action.click_count),
                        audit_detail("display_geometry", display_geometry(&action.display)),
                    ],
                    None,
                    Some("display_identity_and_geometry_revalidated"),
                    Some(recovery),
                ),
            ))
        }
        "computer_drag" => {
            let action = parse_drag(arguments)?;
            Ok((
                operation.to_string(),
                vec![
                    format!("--display-index={}", action.display.index),
                    format!("--start-x={}", action.start_x),
                    format!("--start-y={}", action.start_y),
                    format!("--end-x={}", action.end_x),
                    format!("--end-y={}", action.end_y),
                    format!("--duration-ms={}", action.duration_ms),
                    display_approval_argument(&action.display)?,
                ],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("display_index", action.display.index),
                        audit_detail("display_id", action.display.display_id),
                        audit_detail("start_point", format_point(action.start_x, action.start_y)),
                        audit_detail("end_point", format_point(action.end_x, action.end_y)),
                        audit_detail("duration_ms", action.duration_ms),
                        audit_detail("display_geometry", display_geometry(&action.display)),
                    ],
                    None,
                    Some("display_identity_and_geometry_revalidated"),
                    Some("post_action_observation_and_mouse_up_recovery"),
                ),
            ))
        }
        "computer_press_key" => {
            let action = parse_key_action(arguments)?;
            let mut details = vec![
                audit_detail("key", action.key),
                audit_detail(
                    "modifiers",
                    if action.modifiers.is_empty() {
                        "none".to_string()
                    } else {
                        action.modifiers.join("+")
                    },
                ),
            ];
            if let Some(risk) = key_confirmation_risk(&action) {
                details.push(audit_detail("confirmation_risk", risk));
            }
            Ok((
                operation.to_string(),
                vec![
                    format!("--key={}", action.key),
                    format!("--modifiers={}", action.modifiers.join("+")),
                ],
                computer_use_audit(
                    operation,
                    details,
                    None,
                    Some("reviewed_navigation_key_allowlist"),
                    Some("post_action_observation_and_key_up_recovery"),
                ),
            ))
        }
        "computer_type_text" => {
            let action = parse_typed_text(arguments)?;
            Ok((
                operation.to_string(),
                vec![format!(
                    "--text-json={}",
                    serde_json::to_string(action.text)?
                )],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("target", "focused_non_secure_editable_control"),
                        audit_detail("character_count", action.character_count),
                        audit_detail("utf16_units", action.utf16.len()),
                        audit_detail("text_sha256", action.sha256.clone()),
                        audit_detail("confirmation_risk", "sensitive_text_entry"),
                    ],
                    Some("text_redacted_from_persistent_history"),
                    Some("focused_target_identity_and_editability_revalidated_before_input"),
                    Some("post_action_observation_and_key_up_recovery"),
                ),
            ))
        }
        "computer_scroll" => {
            let action = parse_scroll(arguments)?;
            Ok((
                operation.to_string(),
                vec![
                    format!("--delta-y={}", action.delta_y),
                    format!("--delta-x={}", action.delta_x),
                ],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("delta_y", action.delta_y),
                        audit_detail("delta_x", action.delta_x),
                        audit_detail("target", "current_pointer_target"),
                    ],
                    None,
                    Some("bounded_single_scroll_event"),
                    Some("post_action_observation_before_retry"),
                ),
            ))
        }
        "computer_activate_application" => {
            let pid = parse_application_pid(arguments)?;
            let identity = lookup_application(pid)?;
            let application = identity
                .get("application")
                .and_then(Value::as_str)
                .map(safe_approval_label)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Unnamed application".to_string());
            Ok((
                operation.to_string(),
                vec![
                    format!("--pid={pid}"),
                    format!(
                        "--application-json={}",
                        serde_json::to_string(&application)?
                    ),
                ],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("pid", pid),
                        audit_detail("application", application),
                    ],
                    None,
                    Some("process_identity_revalidated_before_activation"),
                    Some("post_action_observation_before_retry"),
                ),
            ))
        }
        "computer_set_frontmost_window_bounds" => {
            let request = parse_window_bounds_request(arguments)?;
            let display_layout = active_display_layout_guard()?;
            validate_requested_window_bounds_against_layout(&request, &display_layout)?;
            let target = frontmost_window_control_target()?;
            validate_window_bounds_capability(&target)?;
            Ok((
                operation.to_string(),
                vec![
                    format!("--x={}", request.x),
                    format!("--y={}", request.y),
                    format!("--width={}", request.width),
                    format!("--height={}", request.height),
                    window_approval_argument(&target)?,
                    window_display_layout_approval_argument(&display_layout)?,
                ],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("application", safe_approval_label(&target.application)),
                        audit_detail("pid", target.pid),
                        audit_detail("window_id", &target.window_id),
                        audit_detail("original_geometry", target.geometry()),
                        audit_detail("target_geometry", request.geometry()),
                    ],
                    None,
                    Some("frontmost_window_identity_state_geometry_and_display_layout_revalidated"),
                    Some(
                        "identity_bound_window_geometry_restore_on_partial_failure_or_cancellation",
                    ),
                ),
            ))
        }
        "computer_set_frontmost_window_fullscreen" => {
            let fullscreen = parse_window_fullscreen_request(arguments)?;
            let target = frontmost_window_control_target()?;
            validate_window_fullscreen_capability(&target, fullscreen)?;
            Ok((
                operation.to_string(),
                vec![
                    format!("--fullscreen={fullscreen}"),
                    window_approval_argument(&target)?,
                ],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("application", safe_approval_label(&target.application)),
                        audit_detail("pid", target.pid),
                        audit_detail("window_id", &target.window_id),
                        audit_detail("original_fullscreen", target.fullscreen.unwrap_or(false)),
                        audit_detail("target_fullscreen", fullscreen),
                        audit_detail("original_geometry", target.geometry()),
                    ],
                    None,
                    Some("macos_ax_fullscreen_identity_and_state_revalidated"),
                    Some("identity_bound_fullscreen_state_restore_on_failure_or_cancellation"),
                ),
            ))
        }
        "computer_set_frontmost_window_maximized" => {
            let maximized = parse_window_maximized_request(arguments)?;
            let target = frontmost_window_control_target()?;
            validate_window_maximized_capability(&target, maximized)?;
            Ok((
                operation.to_string(),
                vec![
                    format!("--maximized={maximized}"),
                    window_approval_argument(&target)?,
                ],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("application", safe_approval_label(&target.application)),
                        audit_detail("pid", target.pid),
                        audit_detail("window_id", &target.window_id),
                        audit_detail("original_maximized", target.maximized.unwrap_or(false)),
                        audit_detail("target_maximized", maximized),
                        audit_detail("original_geometry", target.geometry()),
                    ],
                    None,
                    Some("windows_foreground_hwnd_identity_and_state_revalidated"),
                    Some("identity_bound_maximized_state_restore_on_failure_or_cancellation"),
                ),
            ))
        }
        "computer_restore_window_layout" => {
            let reference = parse_window_layout_reference(arguments)?;
            let snapshot = stored_window_layout_snapshot(&reference)?;
            validate_window_layout_snapshot_for_approval(&snapshot)?;
            Ok((
                operation.to_string(),
                vec![window_layout_approval_argument(&snapshot)?],
                computer_use_audit(
                    operation,
                    vec![
                        audit_detail("snapshot_id", &snapshot.snapshot_id),
                        audit_detail("snapshot_sha256", &snapshot.snapshot_sha256),
                        audit_detail("window_count", snapshot.windows.len()),
                        audit_detail(
                            "applications",
                            window_layout_application_summary(&snapshot.windows),
                        ),
                        audit_detail("confirmation_risk", "multi_window_layout_restore"),
                    ],
                    Some("native_window_identities_and_coordinates_redacted_from_model_request"),
                    Some(
                        "exact_volatile_snapshot_display_and_window_identities_revalidated_before_batch",
                    ),
                    Some(
                        "identity_bound_batch_rollback_without_application_content_rollback",
                    ),
                ),
            ))
        }
        _ => Err(anyhow!(
            "Computer Use operation does not require interactive approval: {operation}"
        )),
    }
}

fn computer_use_audit(
    operation: &str,
    details: Vec<ApprovalActionAuditDetail>,
    privacy: Option<&str>,
    safety: Option<&str>,
    recovery: Option<&str>,
) -> ApprovalActionAudit {
    ApprovalActionAudit {
        kind: "computer_use".to_string(),
        operation: operation.to_string(),
        details,
        privacy: privacy.map(ToOwned::to_owned),
        safety: safety.map(ToOwned::to_owned),
        recovery: recovery.map(ToOwned::to_owned),
    }
}

fn audit_detail(key: &str, value: impl ToString) -> ApprovalActionAuditDetail {
    ApprovalActionAuditDetail {
        key: key.to_string(),
        value: value.to_string(),
    }
}

fn format_point(x: f64, y: f64) -> String {
    format!("{}, {}", format_audit_number(x), format_audit_number(y))
}

pub(super) fn format_audit_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn display_geometry(display: &DisplayTarget) -> String {
    format!(
        "{} x {} @ {}, {}",
        format_audit_number(display.width),
        format_audit_number(display.height),
        format_audit_number(display.origin_x),
        format_audit_number(display.origin_y),
    )
}

pub(super) fn redact_approval_arguments(operation: &str) -> bool {
    matches!(
        operation,
        "computer_type_text" | "computer_restore_window_layout"
    )
}
