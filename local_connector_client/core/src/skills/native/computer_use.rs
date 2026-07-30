// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod action;
mod application_execution;
mod approval;
mod capture;
mod dispatch;
mod display;
#[cfg(target_os = "macos")]
mod helper;
#[cfg(target_os = "macos")]
mod input_guard;
#[cfg(any(target_os = "macos", test))]
mod jxa_application_scripts;
#[cfg(any(target_os = "macos", test))]
mod jxa_observation_scripts;
mod jxa_runtime;
#[cfg(any(target_os = "macos", test))]
mod jxa_window_scripts;
mod key_action;
#[cfg(target_os = "macos")]
mod macos_text_target;
mod observation;
mod observation_model;
mod permissions;
mod pointer_action;
mod scroll_action;
mod text_action;
mod tool_schema;
mod window_control;
mod window_execution;
mod window_layout;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
use tool_schema::tool_definitions_for_platform;

#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::ffi::c_void;
#[cfg(all(test, target_os = "macos"))]
use std::process::{Command, ExitStatus};
use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

use anyhow::{anyhow, Result};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
#[cfg(test)]
use sha2::{Digest, Sha256};

use crate::approval::ApprovalActionAudit;

#[cfg(any(target_os = "windows", test))]
use action::drag_step_count;
#[cfg(any(target_os = "windows", test))]
use action::ClickAction;
#[cfg(test)]
use action::{click_approval_arguments, parse_click_count};
use action::{
    ensure_action_not_cancelled, is_unsafe_typed_character, parse_click, parse_drag,
    parse_key_action, parse_scroll, parse_typed_text,
};
#[cfg(target_os = "windows")]
use action::{DragAction, KeyAction, ScrollAction, TypedTextAction};
pub(in crate::skills::native::computer_use) use application_execution::{
    activate_application_with_rollback, approved_application_name, lookup_application,
    parse_application_pid, rollback_application_activation, ApplicationActivationRollbackGuard,
};
use capture::{capture_display, capture_frontmost_window};
#[cfg(any(target_os = "windows", test))]
use capture::{
    frontmost_window_screenshot_result, screenshot_result, FrontmostWindowCaptureTarget,
};
use display::{
    active_display_layout_guard, current_platform_name, display_approval_argument, list_displays,
    required_display_index, resolve_display, validate_approved_display,
    validate_approved_window_display_layout, validate_requested_window_bounds_against_layout,
    window_display_layout_approval_argument, ApprovedDisplayGuard, DisplayTarget,
};
#[cfg(any(target_os = "macos", test))]
pub(in crate::skills::native::computer_use) use jxa_application_scripts::{
    ACTIVATE_APPLICATION_JXA, FRONTMOST_APPLICATION_JXA, LOOKUP_APPLICATION_JXA,
    RESTORE_APPLICATION_JXA,
};
#[cfg(any(target_os = "macos", test))]
pub(in crate::skills::native::computer_use) use jxa_observation_scripts::{
    CAPTURE_WINDOW_LAYOUT_JXA, FRONTMOST_WINDOW_CAPTURE_TARGET_JXA, INSPECT_FRONTMOST_WINDOW_JXA,
    LIST_WINDOWS_JXA, PREFLIGHT_WINDOW_LAYOUT_JXA, RESTORE_WINDOW_LAYOUT_JXA,
    ROLLBACK_WINDOW_LAYOUT_JXA,
};
#[cfg(test)]
use jxa_runtime::{classify_macos_observer_error, decode_jxa_result};
pub(in crate::skills::native::computer_use) use jxa_runtime::{
    classify_macos_screenshot_error, execute_jxa, execute_jxa_action, join_reader, read_limited,
};
#[cfg(any(target_os = "macos", test))]
pub(in crate::skills::native::computer_use) use jxa_window_scripts::{
    FRONTMOST_WINDOW_CONTROL_TARGET_JXA, RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA,
    RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA, SET_FRONTMOST_WINDOW_BOUNDS_JXA,
    SET_FRONTMOST_WINDOW_FULLSCREEN_JXA,
};
use key_action::press_key;
#[cfg(all(test, target_os = "macos"))]
use macos_text_target::{classify_macos_text_target, MacTextTargetClass};
use observation::{
    attach_activation_post_action_observation, attach_post_action_observation,
    attach_window_post_action_observation,
};
#[cfg(test)]
use observation::{build_post_action_result, with_application_activation_recovery};
use observation_model::{PostActionObservationTarget, WindowControlRollbackGuard};
#[cfg(any(target_os = "windows", test))]
use pointer_action::click_result;
use pointer_action::{click, drag};
use scroll_action::scroll;
use text_action::type_text;
#[cfg(any(target_os = "windows", test))]
use text_action::typed_text_result;
use window_control::{
    approved_window_guard, parse_window_bounds_request, parse_window_fullscreen_request,
    parse_window_maximized_request, validate_window_bounds_capability,
    validate_window_fullscreen_capability, validate_window_maximized_capability,
    window_approval_argument, ApprovedFrontmostWindowGuard, WindowBoundsRequest,
};
pub(in crate::skills::native::computer_use) use window_execution::{
    frontmost_window_control_target, frontmost_window_control_target_local,
    macos_frontmost_window_control_target_local, restore_window_layout,
    rollback_frontmost_window_bounds, rollback_frontmost_window_fullscreen,
    rollback_frontmost_window_maximized, set_frontmost_window_bounds,
    set_frontmost_window_fullscreen, set_frontmost_window_maximized,
};
#[cfg(any(target_os = "windows", test))]
use window_layout::ApprovedWindowLayoutGuard;
use window_layout::{
    approved_window_layout_snapshot, consume_approved_window_layout_snapshot,
    finalize_window_layout_capture, parse_window_layout_reference, stored_window_layout_snapshot,
    validate_approved_window_layout_snapshot, validate_window_layout_snapshot_for_approval,
    window_layout_application_summary, window_layout_approval_argument, WindowLayoutCapturePayload,
    WindowLayoutSnapshot,
};
#[cfg(test)]
use window_layout::{
    evict_window_layout_snapshot_for_insert, prune_expired_window_layout_snapshots,
    store_window_layout_snapshot, window_layout_sha256, StoredWindowLayoutSnapshot,
};

const MACOS_OSASCRIPT_PATH: &str = "/usr/bin/osascript";
const MACOS_SCREENCAPTURE_PATH: &str = "/usr/sbin/screencapture";
const COMPUTER_USE_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const COMPUTER_USE_OUTPUT_MAX_BYTES: usize = 512 * 1024;
const COMPUTER_USE_STDERR_MAX_BYTES: usize = 64 * 1024;
const DEFAULT_WINDOW_LIMIT: u64 = 40;
const MAX_WINDOW_LIMIT: u64 = 100;
const DEFAULT_TREE_DEPTH: u64 = 4;
const MAX_TREE_DEPTH: u64 = 6;
const DEFAULT_TREE_NODES: u64 = 200;
const MAX_TREE_NODES: u64 = 400;
const MAX_TYPED_TEXT_CHARS: usize = 256;
const MAX_TYPED_TEXT_UTF16_UNITS: usize = 512;
const MAX_SCROLL_DELTA: i64 = 1_200;
const MAX_ACTIVE_DISPLAYS: usize = 16;
const DEFAULT_DRAG_DURATION_MS: u64 = 300;
const MIN_DRAG_DURATION_MS: u64 = 80;
const MAX_DRAG_DURATION_MS: u64 = 1_000;
const MAX_DRAG_STEPS: u32 = 60;
const MIN_WINDOW_DIMENSION: i64 = 64;
const MAX_WINDOW_DIMENSION: i64 = 32_768;
const MIN_WINDOW_COORDINATE: i64 = -100_000;
const MAX_WINDOW_COORDINATE: i64 = 100_000;
const POST_ACTION_SETTLE_DELAY: Duration = Duration::from_millis(160);
const MAX_WINDOW_LAYOUT_WINDOWS: usize = 8;
const MAX_WINDOW_LAYOUT_SNAPSHOTS: usize = 8;
const WINDOW_LAYOUT_SNAPSHOT_TTL: Duration = Duration::from_secs(10 * 60);
const WINDOW_LAYOUT_SCHEMA_VERSION: u32 = 1;

const CONTROL_OPERATIONS: [&str; 10] = [
    "computer_click",
    "computer_drag",
    "computer_press_key",
    "computer_type_text",
    "computer_scroll",
    "computer_activate_application",
    "computer_set_frontmost_window_bounds",
    "computer_set_frontmost_window_fullscreen",
    "computer_set_frontmost_window_maximized",
    "computer_restore_window_layout",
];

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> *const c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> i32;
    fn AXUIElementGetPid(element: *const c_void, pid: *mut i32) -> i32;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementIsAttributeSettable(
        element: *const c_void,
        attribute: *const c_void,
        settable: *mut u8,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, timeout_in_seconds: f32) -> i32;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetType(value: *const c_void) -> u32;
    fn AXValueGetValue(value: *const c_void, value_type: u32, output: *mut c_void) -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut u32,
        display_count: *mut u32,
    ) -> i32;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGDisplayRotation(display: u32) -> f64;
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> *mut c_void;
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CGEventSetLocation(event: *mut c_void, location: CGPoint);
    fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventCreateScrollWheelEvent2(
        source: *const c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> *mut c_void;
    fn CGEventKeyboardSetUnicodeString(
        event: *mut c_void,
        string_length: usize,
        unicode_string: *const u16,
    );
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: *const c_void) -> u8;
    fn CFEqual(left: *const c_void, right: *const c_void) -> u8;
    fn CFGetTypeID(value: *const c_void) -> usize;
    fn CFRetain(value: *const c_void) -> *const c_void;
    fn CFRelease(value: *const c_void);
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        byte_count: isize,
        encoding: u32,
        is_external_representation: u8,
    ) -> *const c_void;
    fn CFStringGetTypeID() -> usize;
}

pub(super) fn tool_definitions(include_control: bool) -> Vec<Value> {
    tool_schema::tool_definitions(include_control)
}

pub(super) fn requires_interactive_approval(operation: &str) -> bool {
    approval::requires_interactive_approval(operation)
}

pub(super) fn approval_command(
    operation: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>, ApprovalActionAudit)> {
    approval::approval_command(operation, arguments)
}

pub(super) fn redact_approval_arguments(operation: &str) -> bool {
    approval::redact_approval_arguments(operation)
}

pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    dispatch::execute(operation, arguments)
}

pub(super) fn execute_approved(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    dispatch::execute_approved(
        operation,
        arguments,
        approved_command_args,
        action_cancelled,
    )
}

pub(super) fn dependency_error() -> Option<String> {
    permissions::dependency_error()
}

pub(super) fn screen_capture_dependency_error() -> Option<String> {
    permissions::screen_capture_dependency_error()
}

pub(super) fn request_permission(permission_id: &str) -> Result<bool> {
    permissions::request_permission(permission_id)
}

fn bounded_signed_integer(
    arguments: &Value,
    field: &str,
    default_value: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64> {
    let value = arguments
        .get(field)
        .map(|value| {
            value
                .as_i64()
                .ok_or_else(|| anyhow!("{field} must be an integer"))
        })
        .transpose()?
        .unwrap_or(default_value);
    if !(minimum..=maximum).contains(&value) {
        return Err(anyhow!("{field} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

fn reject_unknown_fields(arguments: &Value, allowed: &[&str]) -> Result<()> {
    let map = arguments
        .as_object()
        .ok_or_else(|| anyhow!("Computer Use arguments must be an object"))?;
    if let Some(field) = map.keys().find(|field| !allowed.contains(&field.as_str())) {
        return Err(anyhow!("unsupported Computer Use argument: {field}"));
    }
    Ok(())
}

fn finite_number(arguments: &Value, field: &str) -> Result<f64> {
    let value = arguments
        .get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("{field} must be a number"))?;
    if !value.is_finite() {
        return Err(anyhow!("{field} must be finite"));
    }
    Ok(value)
}

fn key_code(key: &str) -> Result<u16> {
    match key {
        "enter" => Ok(36),
        "tab" => Ok(48),
        "space" => Ok(49),
        "backspace" => Ok(51),
        "escape" => Ok(53),
        "home" => Ok(115),
        "page_up" => Ok(116),
        "end" => Ok(119),
        "page_down" => Ok(121),
        "left" => Ok(123),
        "right" => Ok(124),
        "down" => Ok(125),
        "up" => Ok(126),
        _ => Err(anyhow!("unsupported reviewed key: {key}")),
    }
}

fn safe_approval_label(value: &str) -> String {
    value
        .chars()
        .take(120)
        .map(|character| {
            if is_unsafe_typed_character(character) {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub(super) fn run_helper() -> Result<()> {
    helper::run()
}

#[cfg(not(target_os = "macos"))]
pub(super) fn run_helper() -> Result<()> {
    Err(anyhow!("Computer Use helper is only available on macOS"))
}

fn bounded_integer(
    arguments: &Value,
    field: &str,
    default_value: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64> {
    let value = arguments
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| anyhow!("{field} must be an integer"))
        })
        .transpose()?
        .unwrap_or(default_value);
    if !(minimum..=maximum).contains(&value) {
        return Err(anyhow!("{field} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
