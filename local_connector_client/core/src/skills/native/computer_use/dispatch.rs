// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

#[cfg(target_os = "macos")]
use super::helper;
use super::permissions::ensure_observation_runtime;
#[cfg(target_os = "windows")]
use super::windows;
use super::{
    activate_application_with_rollback, active_display_layout_guard, approved_application_name,
    approved_window_guard, approved_window_layout_snapshot,
    attach_activation_post_action_observation, attach_post_action_observation,
    attach_window_post_action_observation, bounded_integer, capture_display,
    capture_frontmost_window, click, consume_approved_window_layout_snapshot, drag,
    ensure_action_not_cancelled, finalize_window_layout_capture, list_displays,
    parse_application_pid, parse_click, parse_drag, parse_key_action, parse_scroll,
    parse_typed_text, parse_window_bounds_request, parse_window_fullscreen_request,
    parse_window_layout_reference, parse_window_maximized_request, press_key,
    reject_unknown_fields, required_display_index, restore_window_layout, scroll,
    set_frontmost_window_bounds, set_frontmost_window_fullscreen, set_frontmost_window_maximized,
    type_text, validate_approved_display, validate_approved_window_display_layout,
    validate_approved_window_layout_snapshot, ApprovedDisplayGuard, PostActionObservationTarget,
    WindowControlRollbackGuard, WindowLayoutCapturePayload, WindowLayoutSnapshot,
    DEFAULT_TREE_DEPTH, DEFAULT_TREE_NODES, DEFAULT_WINDOW_LIMIT, MAX_TREE_DEPTH, MAX_TREE_NODES,
    MAX_WINDOW_LAYOUT_WINDOWS, MAX_WINDOW_LIMIT,
};
#[cfg(target_os = "macos")]
use super::{
    execute_jxa, execute_jxa_action, CAPTURE_WINDOW_LAYOUT_JXA, INSPECT_FRONTMOST_WINDOW_JXA,
    LIST_WINDOWS_JXA, PREFLIGHT_WINDOW_LAYOUT_JXA,
};

#[cfg(target_os = "macos")]
fn list_windows(limit: u64) -> Result<Value> {
    execute_jxa(LIST_WINDOWS_JXA, &[limit.to_string()])
}

#[cfg(target_os = "windows")]
fn list_windows(limit: u64) -> Result<Value> {
    windows::list_windows(limit)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn list_windows(_limit: u64) -> Result<Value> {
    Err(anyhow!(
        "Computer Use window discovery is unsupported on this platform"
    ))
}

fn capture_window_layout_payload() -> Result<Value> {
    let display_layout = active_display_layout_guard()?;
    let mut payload = capture_window_layout_platform()?;
    if active_display_layout_guard()? != display_layout {
        return Err(anyhow!(
            "active display identity or geometry changed during window layout capture"
        ));
    }
    payload.display_layout = display_layout;
    serde_json::to_value(payload).context("encode native window layout capture")
}

#[cfg(target_os = "macos")]
fn capture_window_layout_platform() -> Result<WindowLayoutCapturePayload> {
    let value = execute_jxa_action(
        CAPTURE_WINDOW_LAYOUT_JXA,
        &[MAX_WINDOW_LAYOUT_WINDOWS.to_string()],
    )?;
    let mut payload = serde_json::from_value::<WindowLayoutCapturePayload>(value)
        .context("decode macOS window layout capture")?;
    payload.display_layout.clear();
    Ok(payload)
}

#[cfg(target_os = "windows")]
fn capture_window_layout_platform() -> Result<WindowLayoutCapturePayload> {
    windows::capture_window_layout(MAX_WINDOW_LAYOUT_WINDOWS)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_window_layout_platform() -> Result<WindowLayoutCapturePayload> {
    Err(anyhow!(
        "Computer Use window layout capture is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn preflight_window_layout_snapshot(snapshot: &WindowLayoutSnapshot) -> Result<()> {
    helper::preflight_window_layout(snapshot)
}

#[cfg(target_os = "windows")]
pub(super) fn preflight_window_layout_snapshot(snapshot: &WindowLayoutSnapshot) -> Result<()> {
    preflight_window_layout_snapshot_local(snapshot).map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn preflight_window_layout_snapshot(_snapshot: &WindowLayoutSnapshot) -> Result<()> {
    Err(anyhow!(
        "Computer Use window layout restore is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn preflight_window_layout_snapshot_local(
    snapshot: &WindowLayoutSnapshot,
) -> Result<Value> {
    snapshot.validate()?;
    execute_jxa_action(
        PREFLIGHT_WINDOW_LAYOUT_JXA,
        &[serde_json::to_string(snapshot)?],
    )
}

#[cfg(target_os = "windows")]
pub(super) fn preflight_window_layout_snapshot_local(
    snapshot: &WindowLayoutSnapshot,
) -> Result<Value> {
    windows::preflight_window_layout(snapshot)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn preflight_window_layout_snapshot_local(
    _snapshot: &WindowLayoutSnapshot,
) -> Result<Value> {
    Err(anyhow!(
        "Computer Use window layout restore is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn inspect_frontmost_window(max_depth: u64, max_nodes: u64) -> Result<Value> {
    execute_jxa(
        INSPECT_FRONTMOST_WINDOW_JXA,
        &[max_depth.to_string(), max_nodes.to_string()],
    )
}

#[cfg(target_os = "windows")]
fn inspect_frontmost_window(max_depth: u64, max_nodes: u64) -> Result<Value> {
    windows::inspect_frontmost_window(max_depth, max_nodes)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn inspect_frontmost_window(_max_depth: u64, _max_nodes: u64) -> Result<Value> {
    Err(anyhow!(
        "Computer Use frontmost-window control-tree inspection is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    let result = helper::execute(operation, arguments)?;
    if operation == "computer_capture_window_layout" {
        finalize_window_layout_capture(result)
    } else {
        Ok(result)
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    let result = execute_local(operation, arguments)?;
    if operation == "computer_capture_window_layout" {
        finalize_window_layout_capture(result)
    } else {
        Ok(result)
    }
}

pub(super) fn execute_local(operation: &str, arguments: &Value) -> Result<Value> {
    ensure_observation_runtime()?;
    match operation {
        "computer_list_windows" => {
            let limit = bounded_integer(
                arguments,
                "limit",
                DEFAULT_WINDOW_LIMIT,
                1,
                MAX_WINDOW_LIMIT,
            )?;
            list_windows(limit)
        }
        "computer_capture_window_layout" => {
            reject_unknown_fields(arguments, &[])?;
            capture_window_layout_payload()
        }
        "computer_inspect_frontmost_window" => {
            let max_depth = bounded_integer(
                arguments,
                "max_depth",
                DEFAULT_TREE_DEPTH,
                1,
                MAX_TREE_DEPTH,
            )?;
            let max_nodes = bounded_integer(
                arguments,
                "max_nodes",
                DEFAULT_TREE_NODES,
                1,
                MAX_TREE_NODES,
            )?;
            inspect_frontmost_window(max_depth, max_nodes)
        }
        "computer_capture_main_display" => capture_display(None),
        "computer_capture_frontmost_window" => capture_frontmost_window(),
        "computer_list_displays" => list_displays(),
        "computer_capture_display" => {
            let display_index = required_display_index(arguments)?;
            capture_display(Some(display_index))
        }
        _ => Err(anyhow!(
            "Computer Use operation is not implemented: {operation}"
        )),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn execute_approved(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if operation == "computer_restore_window_layout" {
        consume_approved_window_layout_snapshot(arguments, approved_command_args)?;
    }
    helper::execute_approved(
        operation,
        arguments,
        approved_command_args,
        action_cancelled,
    )
}

#[cfg(not(target_os = "macos"))]
pub(super) fn execute_approved(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if operation == "computer_restore_window_layout" {
        consume_approved_window_layout_snapshot(arguments, approved_command_args)?;
    }
    execute_approved_local(
        operation,
        arguments,
        approved_command_args,
        action_cancelled,
    )
}

pub(super) fn execute_approved_local(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    ensure_observation_runtime()?;
    ensure_action_not_cancelled(action_cancelled)?;
    if operation == "computer_restore_window_layout" {
        let reference = parse_window_layout_reference(arguments)?;
        let snapshot = approved_window_layout_snapshot(approved_command_args)?;
        if snapshot.snapshot_id != reference.snapshot_id
            || snapshot.snapshot_sha256 != reference.snapshot_sha256
        {
            return Err(anyhow!(
                "approved window layout snapshot does not match the requested opaque reference"
            ));
        }
        validate_approved_window_layout_snapshot(&snapshot)?;
        return restore_window_layout(&snapshot, action_cancelled);
    }
    if operation == "computer_activate_application" {
        let (result, rollback_guard) = activate_application_with_rollback(
            parse_application_pid(arguments)?,
            approved_application_name(approved_command_args)?,
            action_cancelled,
        )?;
        return Ok(attach_activation_post_action_observation(
            operation,
            result,
            rollback_guard,
            PostActionObservationTarget::MainDisplay,
            action_cancelled,
        ));
    }
    if operation == "computer_set_frontmost_window_bounds" {
        let request = parse_window_bounds_request(arguments)?;
        validate_approved_window_display_layout(&request, approved_command_args)?;
        let approved = approved_window_guard(approved_command_args)?;
        let result = set_frontmost_window_bounds(request, approved.clone(), action_cancelled)?;
        return Ok(attach_window_post_action_observation(
            operation,
            result,
            WindowControlRollbackGuard::Bounds { request, approved },
            action_cancelled,
        ));
    }
    if operation == "computer_set_frontmost_window_fullscreen" {
        let fullscreen = parse_window_fullscreen_request(arguments)?;
        let approved = approved_window_guard(approved_command_args)?;
        let result =
            set_frontmost_window_fullscreen(fullscreen, approved.clone(), action_cancelled)?;
        return Ok(attach_window_post_action_observation(
            operation,
            result,
            WindowControlRollbackGuard::Fullscreen {
                fullscreen,
                approved,
            },
            action_cancelled,
        ));
    }
    if operation == "computer_set_frontmost_window_maximized" {
        let maximized = parse_window_maximized_request(arguments)?;
        let approved = approved_window_guard(approved_command_args)?;
        let result = set_frontmost_window_maximized(maximized, approved.clone(), action_cancelled)?;
        return Ok(attach_window_post_action_observation(
            operation,
            result,
            WindowControlRollbackGuard::Maximized {
                maximized,
                approved,
            },
            action_cancelled,
        ));
    }
    let (result, observation_target) = match operation {
        "computer_click" => {
            let action = parse_click(arguments)?;
            validate_approved_display(&action.display, approved_command_args)?;
            let display = ApprovedDisplayGuard::from(&action.display);
            (
                click(action, action_cancelled)?,
                PostActionObservationTarget::ApprovedDisplay(display),
            )
        }
        "computer_drag" => {
            let action = parse_drag(arguments)?;
            validate_approved_display(&action.display, approved_command_args)?;
            let display = ApprovedDisplayGuard::from(&action.display);
            (
                drag(action, action_cancelled)?,
                PostActionObservationTarget::ApprovedDisplay(display),
            )
        }
        "computer_press_key" => (
            press_key(parse_key_action(arguments)?)?,
            PostActionObservationTarget::MainDisplay,
        ),
        "computer_type_text" => (
            type_text(parse_typed_text(arguments)?)?,
            PostActionObservationTarget::MainDisplay,
        ),
        "computer_scroll" => (
            scroll(parse_scroll(arguments)?)?,
            PostActionObservationTarget::MainDisplay,
        ),
        _ => return execute_local(operation, arguments),
    };
    Ok(attach_post_action_observation(
        operation,
        result,
        observation_target,
        action_cancelled,
    ))
}
