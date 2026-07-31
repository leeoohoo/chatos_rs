// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;
#[cfg(target_os = "macos")]
use std::sync::atomic::Ordering;

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
use super::helper;
#[cfg(target_os = "windows")]
use super::windows;
#[cfg(target_os = "macos")]
use super::{
    ensure_action_not_cancelled, execute_jxa_action, validate_window_bounds_capability,
    validate_window_fullscreen_capability, FRONTMOST_WINDOW_CONTROL_TARGET_JXA,
    RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA, RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA,
    ROLLBACK_WINDOW_LAYOUT_JXA, SET_FRONTMOST_WINDOW_BOUNDS_JXA,
    SET_FRONTMOST_WINDOW_FULLSCREEN_JXA,
};
use super::{ApprovedFrontmostWindowGuard, WindowBoundsRequest, WindowLayoutSnapshot};

#[cfg(target_os = "macos")]
pub(super) fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    helper::frontmost_window_control_target()
}

#[cfg(target_os = "windows")]
pub(super) fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    windows::frontmost_window_control_target()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    Err(anyhow!(
        "Computer Use frontmost-window control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard> {
    macos_frontmost_window_control_target_local()
}

#[cfg(target_os = "windows")]
pub(super) fn frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard> {
    windows::frontmost_window_control_target()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard> {
    Err(anyhow!(
        "Computer Use frontmost-window control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn restore_window_layout(
    snapshot: &WindowLayoutSnapshot,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    snapshot.validate()?;
    ensure_action_not_cancelled(action_cancelled)?;
    let snapshot_json = serde_json::to_string(snapshot)?;
    let mut result = super::execute_jxa_action(
        super::RESTORE_WINDOW_LAYOUT_JXA,
        std::slice::from_ref(&snapshot_json),
    )?;
    let pre_action_windows = result
        .as_object_mut()
        .and_then(|map| map.remove("pre_action_windows"));
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst))
        && result.get("success").and_then(Value::as_bool) == Some(true)
    {
        let recovery = pre_action_windows
            .as_ref()
            .and_then(|before| serde_json::to_string(before).ok())
            .and_then(|before_json| {
                execute_jxa_action(ROLLBACK_WINDOW_LAYOUT_JXA, &[snapshot_json, before_json]).ok()
            })
            .unwrap_or_else(|| {
                json!({
                    "attempted": true,
                    "restored_count": 0,
                    "skipped_count": 0,
                    "failed_count": snapshot.windows.len(),
                    "complete": false,
                })
            });
        if let Some(map) = result.as_object_mut() {
            map.insert("success".to_string(), Value::Bool(false));
            map.insert(
                "failure_reason".to_string(),
                Value::String("cancelled_after_action".to_string()),
            );
            map.insert("restored_window_count".to_string(), json!(0));
            map.insert("action_already_executed".to_string(), Value::Bool(true));
            map.insert("window_layout_recovery".to_string(), recovery.clone());
            map.insert(
                "manual_review_required".to_string(),
                Value::Bool(recovery.get("complete").and_then(Value::as_bool) != Some(true)),
            );
        }
    }
    Ok(result)
}

#[cfg(target_os = "windows")]
pub(super) fn restore_window_layout(
    snapshot: &WindowLayoutSnapshot,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    windows::restore_window_layout(snapshot, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn restore_window_layout(
    _snapshot: &WindowLayoutSnapshot,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "Computer Use window layout restore is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn set_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    set_macos_frontmost_window_bounds(request, approved, action_cancelled)
}

#[cfg(target_os = "windows")]
pub(super) fn set_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    windows::set_frontmost_window_bounds(request, approved, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn set_frontmost_window_bounds(
    _request: WindowBoundsRequest,
    _approved: ApprovedFrontmostWindowGuard,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "Computer Use frontmost-window bounds control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn rollback_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    let approved_json = serde_json::to_string(approved);
    let request_json = serde_json::to_string(&json!({
        "x": request.x,
        "y": request.y,
        "width": request.width,
        "height": request.height,
    }));
    match (approved_json, request_json) {
        (Ok(approved_json), Ok(request_json)) => execute_jxa_action(
            RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA,
            &[approved_json, request_json],
        )
        .unwrap_or_else(
            |_| json!({"attempted": true, "restored": false, "reason": "platform_restore_failed"}),
        ),
        _ => {
            json!({"attempted": false, "restored": false, "reason": "approved_restore_context_invalid"})
        }
    }
}

#[cfg(target_os = "windows")]
pub(super) fn rollback_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    windows::rollback_frontmost_window_bounds(request, approved)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn rollback_frontmost_window_bounds(
    _request: WindowBoundsRequest,
    _approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    json!({"attempted": false, "restored": false, "reason": "platform_restore_unavailable"})
}

#[cfg(target_os = "macos")]
pub(super) fn set_frontmost_window_fullscreen(
    fullscreen: bool,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    set_macos_frontmost_window_fullscreen(fullscreen, approved, action_cancelled)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn set_frontmost_window_fullscreen(
    _fullscreen: bool,
    _approved: ApprovedFrontmostWindowGuard,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "native frontmost-window fullscreen control is available only on macOS"
    ))
}

#[cfg(target_os = "macos")]
pub(super) fn rollback_frontmost_window_fullscreen(
    fullscreen: bool,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    match serde_json::to_string(approved) {
        Ok(approved_json) => execute_jxa_action(
            RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA,
            &[approved_json, fullscreen.to_string()],
        )
        .unwrap_or_else(
            |_| json!({"attempted": true, "restored": false, "reason": "platform_restore_failed"}),
        ),
        Err(_) => {
            json!({"attempted": false, "restored": false, "reason": "approved_restore_context_invalid"})
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn rollback_frontmost_window_fullscreen(
    _fullscreen: bool,
    _approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    json!({"attempted": false, "restored": false, "reason": "platform_restore_unavailable"})
}

#[cfg(target_os = "windows")]
pub(super) fn set_frontmost_window_maximized(
    maximized: bool,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    windows::set_frontmost_window_maximized(maximized, approved, action_cancelled)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn set_frontmost_window_maximized(
    _maximized: bool,
    _approved: ApprovedFrontmostWindowGuard,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "frontmost-window maximize control is available only on Windows"
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn rollback_frontmost_window_maximized(
    maximized: bool,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    windows::rollback_frontmost_window_maximized(maximized, approved)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn rollback_frontmost_window_maximized(
    _maximized: bool,
    _approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    json!({"attempted": false, "restored": false, "reason": "platform_restore_unavailable"})
}

#[cfg(target_os = "macos")]
pub(super) fn macos_frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard>
{
    let value = execute_jxa_action(FRONTMOST_WINDOW_CONTROL_TARGET_JXA, &[])?;
    let target = serde_json::from_value::<ApprovedFrontmostWindowGuard>(value)
        .context("decode macOS frontmost window control target")?;
    target.validate()?;
    Ok(target)
}

#[cfg(target_os = "macos")]
fn set_macos_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    validate_window_bounds_capability(&approved)?;
    ensure_action_not_cancelled(action_cancelled)?;
    let approved_json = serde_json::to_string(&approved)?;
    let request_json = serde_json::to_string(&json!({
        "x": request.x,
        "y": request.y,
        "width": request.width,
        "height": request.height,
    }))?;
    let mut result = execute_jxa_action(
        SET_FRONTMOST_WINDOW_BOUNDS_JXA,
        &[approved_json.clone(), request_json.clone()],
    )?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst))
        && result
            .get("target_geometry_applied")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        let recovery = execute_jxa_action(
            RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA,
            &[approved_json, request_json],
        )
        .unwrap_or_else(|_| {
            json!({
                "attempted": true,
                "restored": false,
                "reason": "platform_restore_failed",
            })
        });
        if let Some(map) = result.as_object_mut() {
            map.insert("success".to_string(), Value::Bool(false));
            map.insert("target_geometry_applied".to_string(), Value::Bool(false));
            map.insert(
                "failure_reason".to_string(),
                Value::String("cancelled_after_action".to_string()),
            );
            map.insert("window_geometry_recovery".to_string(), recovery);
        }
    }
    Ok(result)
}

#[cfg(target_os = "macos")]
fn set_macos_frontmost_window_fullscreen(
    fullscreen: bool,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    validate_window_fullscreen_capability(&approved, fullscreen)?;
    ensure_action_not_cancelled(action_cancelled)?;
    let approved_json = serde_json::to_string(&approved)?;
    let requested = fullscreen.to_string();
    let mut result = execute_jxa_action(
        SET_FRONTMOST_WINDOW_FULLSCREEN_JXA,
        &[approved_json.clone(), requested.clone()],
    )?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst))
        && result
            .get("target_fullscreen_applied")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        let recovery = execute_jxa_action(
            RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA,
            &[approved_json, requested],
        )
        .unwrap_or_else(|_| {
            json!({
                "attempted": true,
                "restored": false,
                "reason": "platform_restore_failed",
            })
        });
        if let Some(map) = result.as_object_mut() {
            map.insert("success".to_string(), Value::Bool(false));
            map.insert("target_fullscreen_applied".to_string(), Value::Bool(false));
            map.insert(
                "failure_reason".to_string(),
                Value::String("cancelled_after_action".to_string()),
            );
            map.insert("window_state_recovery".to_string(), recovery);
        }
    }
    Ok(result)
}
