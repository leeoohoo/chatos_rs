// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;
#[cfg(target_os = "macos")]
use std::{thread, time::Duration};

#[cfg(not(target_os = "windows"))]
use anyhow::anyhow;
use anyhow::Result;
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
use super::action::{drag_step_count, ensure_action_not_cancelled};
use super::action::{ClickAction, DragAction};
#[cfg(target_os = "macos")]
use super::input_guard::CoreGraphicsUpGuard;
#[cfg(target_os = "macos")]
use super::{
    CFRelease, CGEventCreateMouseEvent, CGEventPost, CGEventSetIntegerValueField, CGPoint,
};

#[cfg(target_os = "macos")]
pub(super) fn click(
    action: ClickAction<'_>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    const CG_HID_EVENT_TAP: u32 = 0;
    const LEFT_MOUSE_DOWN: u32 = 1;
    const LEFT_MOUSE_UP: u32 = 2;
    const RIGHT_MOUSE_DOWN: u32 = 3;
    const RIGHT_MOUSE_UP: u32 = 4;
    const MOUSE_EVENT_CLICK_STATE: u32 = 1;
    const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(60);
    ensure_action_not_cancelled(action_cancelled)?;
    let (down_type, up_type, button) = if action.button == "right" {
        (RIGHT_MOUSE_DOWN, RIGHT_MOUSE_UP, 1)
    } else {
        (LEFT_MOUSE_DOWN, LEFT_MOUSE_UP, 0)
    };
    let point = CGPoint {
        x: action.global_x,
        y: action.global_y,
    };
    for click_index in 1..=action.click_count {
        ensure_action_not_cancelled(action_cancelled)?;
        // SAFETY: CoreGraphics copies the point and returns retained events. The up event is
        // guarded before down is posted, so later returns still post and release it.
        let (down, up) = unsafe {
            (
                CGEventCreateMouseEvent(std::ptr::null(), down_type, point, button),
                CGEventCreateMouseEvent(std::ptr::null(), up_type, point, button),
            )
        };
        if down.is_null() || up.is_null() {
            // SAFETY: each non-null event is retained and has not been posted.
            unsafe {
                if !down.is_null() {
                    CFRelease(down);
                }
                if !up.is_null() {
                    CFRelease(up);
                }
            }
            return Err(anyhow!("macOS could not create the approved mouse event"));
        }
        if action.click_count == 2 {
            // SAFETY: both events are retained and still owned by this scope.
            unsafe {
                CGEventSetIntegerValueField(down, MOUSE_EVENT_CLICK_STATE, i64::from(click_index));
                CGEventSetIntegerValueField(up, MOUSE_EVENT_CLICK_STATE, i64::from(click_index));
            }
        }
        let mouse_up = CoreGraphicsUpGuard::new(up);
        // SAFETY: down is retained, posted synchronously, and released exactly once.
        unsafe {
            CGEventPost(CG_HID_EVENT_TAP, down);
            CFRelease(down);
        }
        mouse_up.release();
        if click_index < action.click_count {
            thread::sleep(DOUBLE_CLICK_INTERVAL);
            ensure_action_not_cancelled(action_cancelled)?;
        }
    }
    Ok(click_result(&action))
}

pub(super) fn click_result(action: &ClickAction<'_>) -> Value {
    json!({
        "success": true,
        "mode": "approved_input",
        "action": "click",
        "display_index": action.display.index,
        "display_id": action.display.display_id,
        "x": action.x,
        "y": action.y,
        "button": action.button,
        "click_count": action.click_count,
        "interruptible_between_clicks": action.click_count == 2,
    })
}

#[cfg(target_os = "macos")]
pub(super) fn drag(action: DragAction, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    const CG_HID_EVENT_TAP: u32 = 0;
    const LEFT_MOUSE_DOWN: u32 = 1;
    const LEFT_MOUSE_UP: u32 = 2;
    const LEFT_MOUSE_DRAGGED: u32 = 6;
    const LEFT_MOUSE_BUTTON: u32 = 0;
    ensure_action_not_cancelled(action_cancelled)?;
    let start = CGPoint {
        x: action.global_start_x,
        y: action.global_start_y,
    };
    let end = CGPoint {
        x: action.global_end_x,
        y: action.global_end_y,
    };
    // SAFETY: CoreGraphics copies both points by value. The up event is guarded before down is
    // posted, so every later return path posts and releases mouse-up exactly once.
    let (down, up) = unsafe {
        (
            CGEventCreateMouseEvent(std::ptr::null(), LEFT_MOUSE_DOWN, start, LEFT_MOUSE_BUTTON),
            CGEventCreateMouseEvent(std::ptr::null(), LEFT_MOUSE_UP, start, LEFT_MOUSE_BUTTON),
        )
    };
    if down.is_null() || up.is_null() {
        // SAFETY: each non-null pointer is a retained event returned above and has not been posted.
        unsafe {
            if !down.is_null() {
                CFRelease(down);
            }
            if !up.is_null() {
                CFRelease(up);
            }
        }
        return Err(anyhow!("macOS could not create the approved drag events"));
    }
    let mut mouse_up = CoreGraphicsUpGuard::new(up);
    // SAFETY: down is retained, posted synchronously, then released once.
    unsafe {
        CGEventPost(CG_HID_EVENT_TAP, down);
        CFRelease(down);
    }
    let steps = drag_step_count(action.duration_ms);
    let interval = Duration::from_millis((action.duration_ms / u64::from(steps)).max(1));
    for step in 1..=steps {
        ensure_action_not_cancelled(action_cancelled)?;
        thread::sleep(interval);
        ensure_action_not_cancelled(action_cancelled)?;
        let progress = f64::from(step) / f64::from(steps);
        let point = CGPoint {
            x: start.x + (end.x - start.x) * progress,
            y: start.y + (end.y - start.y) * progress,
        };
        // SAFETY: CoreGraphics returns a retained event, which is posted and released once.
        let movement = unsafe {
            CGEventCreateMouseEvent(
                std::ptr::null(),
                LEFT_MOUSE_DRAGGED,
                point,
                LEFT_MOUSE_BUTTON,
            )
        };
        if movement.is_null() {
            return Err(anyhow!("macOS could not continue the approved drag"));
        }
        // SAFETY: movement is a retained event returned immediately above.
        unsafe {
            CGEventPost(CG_HID_EVENT_TAP, movement);
            CFRelease(movement);
        }
        mouse_up.set_location(point);
    }
    mouse_up.set_location(end);
    mouse_up.release();
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "drag",
        "display_index": action.display.index,
        "display_id": action.display.display_id,
        "start_x": action.start_x,
        "start_y": action.start_y,
        "end_x": action.end_x,
        "end_y": action.end_y,
        "duration_ms": action.duration_ms,
        "steps": steps,
        "interruptible": true,
        "mouse_up_guaranteed": true,
    }))
}

#[cfg(target_os = "windows")]
pub(super) fn click(
    action: ClickAction<'_>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    super::windows::click(action, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn click(
    _action: ClickAction<'_>,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn drag(action: DragAction, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    super::windows::drag(action, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn drag(_action: DragAction, _action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}
