// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::observation_model::{PostActionObservationTarget, WindowControlRollbackGuard};
use super::{
    capture_display, capture_frontmost_window, frontmost_window_control_target_local,
    rollback_application_activation, rollback_frontmost_window_bounds,
    rollback_frontmost_window_fullscreen, rollback_frontmost_window_maximized,
    ApplicationActivationRollbackGuard, POST_ACTION_SETTLE_DELAY,
};

pub(super) fn attach_post_action_observation(
    operation: &str,
    action_result: Value,
    target: PostActionObservationTarget,
    action_cancelled: Option<&AtomicBool>,
) -> Value {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_post_action_result(
            operation,
            action_result,
            &target,
            Err("cancelled_after_action"),
        );
    }
    thread::sleep(POST_ACTION_SETTLE_DELAY);
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_post_action_result(
            operation,
            action_result,
            &target,
            Err("cancelled_after_action"),
        );
    }
    let observation = capture_display(target.requested_index())
        .map_err(|error| classify_post_action_observation_error(error.to_string().as_str()));
    build_post_action_result(operation, action_result, &target, observation)
}

pub(super) fn attach_activation_post_action_observation(
    operation: &str,
    action_result: Value,
    rollback_guard: ApplicationActivationRollbackGuard,
    target: PostActionObservationTarget,
    action_cancelled: Option<&AtomicBool>,
) -> Value {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_cancelled_activation_result(
            operation,
            action_result,
            &rollback_guard,
            &target,
        );
    }
    thread::sleep(POST_ACTION_SETTLE_DELAY);
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_cancelled_activation_result(
            operation,
            action_result,
            &rollback_guard,
            &target,
        );
    }
    let observation = capture_display(target.requested_index())
        .map_err(|error| classify_post_action_observation_error(error.to_string().as_str()));
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_cancelled_activation_result(
            operation,
            action_result,
            &rollback_guard,
            &target,
        );
    }
    build_post_action_result(
        operation,
        with_application_activation_recovery(
            action_result,
            json!({
                "scope": "frontmost_application_activation_only",
                "rollback_on_in_flight_cancel": true,
                "attempted": false,
                "restored": false,
                "reason": "action_completed_without_cancellation",
                "application_content_rollback": false,
                "window_geometry_rollback": false,
            }),
        ),
        &target,
        observation,
    )
}

pub(super) fn attach_window_post_action_observation(
    operation: &str,
    action_result: Value,
    rollback_guard: WindowControlRollbackGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Value {
    let target = rollback_guard.observation_target();
    let require_applied_state = window_control_target_was_applied(&action_result);
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_cancelled_window_result(operation, action_result, &rollback_guard, &target);
    }
    thread::sleep(POST_ACTION_SETTLE_DELAY);
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_cancelled_window_result(operation, action_result, &rollback_guard, &target);
    }
    let observation = capture_window_control_observation(&rollback_guard, require_applied_state)
        .map_err(|error| classify_post_action_observation_error(error.to_string().as_str()));
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return build_cancelled_window_result(operation, action_result, &rollback_guard, &target);
    }
    build_post_action_result(operation, action_result, &target, observation)
}

fn capture_window_control_observation(
    rollback_guard: &WindowControlRollbackGuard,
    require_applied_state: bool,
) -> Result<Value> {
    let before = frontmost_window_control_target_local()?;
    let matches_before = if require_applied_state {
        rollback_guard.matches_applied_state(&before)
    } else {
        rollback_guard.matches_target_identity(&before)
    };
    if !matches_before {
        return Err(anyhow!(
            "frontmost window identity or target state changed before post-action capture"
        ));
    }
    let screenshot = capture_frontmost_window()?;
    let after = frontmost_window_control_target_local()?;
    let matches_after = if require_applied_state {
        rollback_guard.matches_applied_state(&after)
    } else {
        rollback_guard.matches_target_identity(&after)
    };
    if !matches_after {
        return Err(anyhow!(
            "frontmost window identity or target state changed during post-action capture"
        ));
    }
    Ok(screenshot)
}

fn build_cancelled_window_result(
    operation: &str,
    mut action_result: Value,
    rollback_guard: &WindowControlRollbackGuard,
    target: &PostActionObservationTarget,
) -> Value {
    if window_control_target_was_applied(&action_result) {
        let recovery = rollback_window_control(rollback_guard);
        if let Some(map) = action_result.as_object_mut() {
            map.insert("success".to_string(), Value::Bool(false));
            map.insert(
                "failure_reason".to_string(),
                Value::String("cancelled_after_action".to_string()),
            );
            match rollback_guard {
                WindowControlRollbackGuard::Bounds { .. } => {
                    map.insert("target_geometry_applied".to_string(), Value::Bool(false));
                    map.insert("window_geometry_recovery".to_string(), recovery);
                }
                WindowControlRollbackGuard::Fullscreen { .. } => {
                    map.insert("target_fullscreen_applied".to_string(), Value::Bool(false));
                    map.insert("window_state_recovery".to_string(), recovery);
                }
                WindowControlRollbackGuard::Maximized { .. } => {
                    map.insert("target_maximized_applied".to_string(), Value::Bool(false));
                    map.insert("window_state_recovery".to_string(), recovery);
                }
            }
        }
    }
    build_post_action_result(
        operation,
        action_result,
        target,
        Err("cancelled_after_action"),
    )
}

fn window_control_target_was_applied(result: &Value) -> bool {
    [
        "target_geometry_applied",
        "target_fullscreen_applied",
        "target_maximized_applied",
    ]
    .iter()
    .any(|field| result.get(*field).and_then(Value::as_bool) == Some(true))
}

fn rollback_window_control(guard: &WindowControlRollbackGuard) -> Value {
    match guard {
        WindowControlRollbackGuard::Bounds { request, approved } => {
            rollback_frontmost_window_bounds(*request, approved)
        }
        WindowControlRollbackGuard::Fullscreen {
            fullscreen,
            approved,
        } => rollback_frontmost_window_fullscreen(*fullscreen, approved),
        WindowControlRollbackGuard::Maximized {
            maximized,
            approved,
        } => rollback_frontmost_window_maximized(*maximized, approved),
    }
}

fn build_cancelled_activation_result(
    operation: &str,
    action_result: Value,
    rollback_guard: &ApplicationActivationRollbackGuard,
    target: &PostActionObservationTarget,
) -> Value {
    let rollback = rollback_application_activation(rollback_guard).unwrap_or_else(|error| {
        json!({
            "scope": "frontmost_application_activation_only",
            "rollback_on_in_flight_cancel": true,
            "attempted": true,
            "restored": false,
            "reason": "rollback_failed",
            "error_class": classify_application_rollback_error(error.to_string().as_str()),
            "application_content_rollback": false,
            "window_geometry_rollback": false,
        })
    });
    build_post_action_result(
        operation,
        with_application_activation_recovery(action_result, rollback),
        target,
        Err("cancelled_after_action"),
    )
}

pub(super) fn with_application_activation_recovery(mut result: Value, rollback: Value) -> Value {
    if let Some(map) = result.as_object_mut() {
        map.insert("application_state_recovery".to_string(), rollback);
    }
    result
}

fn classify_application_rollback_error(message: &str) -> &'static str {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("identity") || normalized.contains("foreground") {
        "application_identity_unavailable"
    } else if normalized.contains("refused") || normalized.contains("policy") {
        "platform_restore_refused"
    } else {
        "rollback_unavailable"
    }
}

pub(super) fn build_post_action_result(
    operation: &str,
    action_result: Value,
    target: &PostActionObservationTarget,
    observation: std::result::Result<Value, &'static str>,
) -> Value {
    let mut structured = action_result.as_object().cloned().unwrap_or_default();
    let action_succeeded = structured
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    structured.insert(
        "recovery".to_string(),
        json!({
            "action_already_executed": true,
            "automatic_replay_safe": false,
            "observe_before_retry": true,
            "input_release_contract": input_release_contract(operation),
        }),
    );
    match observation {
        Ok(mut screenshot) => {
            let capture = screenshot
                .get("_structured_result")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !target.matches_capture(&capture) {
                structured.insert(
                    "post_action_observation".to_string(),
                    post_action_observation_failure(target, target.mismatch_reason()),
                );
                return json!({
                    "text": "The approved Computer Use action completed, but its post-action capture target identity changed. Do not replay the action automatically; observe the desktop again before deciding what to do next.",
                    "_structured_result": Value::Object(structured),
                });
            }
            structured.insert(
                "post_action_observation".to_string(),
                json!({
                    "attempted": true,
                    "captured": true,
                    "persisted": false,
                    "target": target.metadata(),
                    "capture": capture,
                    "refresh_accessibility_tree_before_ref_based_action": true,
                }),
            );
            let model_input = screenshot
                .as_object_mut()
                .and_then(|map| map.remove("_model_input"))
                .unwrap_or_else(|| Value::Array(Vec::new()));
            json!({
                "text": if action_succeeded {
                    "The approved Computer Use action completed. A transient post-action screenshot is attached for recovery and the next model step; its pixels are not persisted."
                } else {
                    "The approved Computer Use action ran, but the requested final state was not retained. Review the identity-bound recovery metadata and transient screenshot before deciding what to do next; never replay the action automatically."
                },
                "_structured_result": Value::Object(structured),
                "_model_input": model_input,
            })
        }
        Err(reason) => {
            structured.insert(
                "post_action_observation".to_string(),
                post_action_observation_failure(target, reason),
            );
            json!({
                "text": if action_succeeded {
                    "The approved Computer Use action completed, but the automatic post-action screenshot was unavailable. Do not replay the action automatically; observe the desktop again before deciding whether another action is needed."
                } else {
                    "The approved Computer Use action ran without retaining the requested final state, and the automatic post-action screenshot was unavailable. Review the recovery metadata, observe again, and do not replay automatically."
                },
                "_structured_result": Value::Object(structured),
            })
        }
    }
}

fn post_action_observation_failure(
    target: &PostActionObservationTarget,
    reason: &'static str,
) -> Value {
    let recommended_tools = match target {
        PostActionObservationTarget::MainDisplay => {
            json!([
                "computer_capture_main_display",
                "computer_capture_frontmost_window",
                "computer_inspect_frontmost_window"
            ])
        }
        PostActionObservationTarget::ApprovedDisplay(_) => json!([
            "computer_list_displays",
            "computer_capture_display",
            "computer_capture_frontmost_window",
            "computer_inspect_frontmost_window"
        ]),
        PostActionObservationTarget::FrontmostWindow(_) => json!([
            "computer_capture_frontmost_window",
            "computer_inspect_frontmost_window",
            "computer_list_windows"
        ]),
    };
    json!({
        "attempted": reason != "cancelled_after_action",
        "captured": false,
        "persisted": false,
        "target": target.metadata(),
        "reason": reason,
        "action_already_executed": true,
        "automatic_replay_safe": false,
        "recommended_tools": recommended_tools,
    })
}

fn classify_post_action_observation_error(message: &str) -> &'static str {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("permission") || normalized.contains("screen recording") {
        "screen_capture_permission_unavailable"
    } else if normalized.contains("timed out") {
        "capture_timeout"
    } else if normalized.contains("frontmost window") || normalized.contains("target state") {
        "frontmost_window_identity_or_state_changed"
    } else if normalized.contains("display") {
        "display_unavailable"
    } else {
        "capture_unavailable"
    }
}

fn input_release_contract(operation: &str) -> &'static str {
    match operation {
        "computer_click" | "computer_drag" => "paired_mouse_up_guard",
        "computer_press_key" | "computer_type_text" => "paired_key_up_recovery",
        _ => "no_latched_input_state",
    }
}
