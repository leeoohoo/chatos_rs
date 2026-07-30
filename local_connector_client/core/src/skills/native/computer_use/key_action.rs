// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(not(target_os = "windows"))]
use anyhow::anyhow;
use anyhow::Result;
use serde_json::{json, Value};

use super::action::KeyAction;
#[cfg(target_os = "macos")]
use super::input_guard::CoreGraphicsUpGuard;
#[cfg(target_os = "macos")]
use super::{key_code, CFRelease, CGEventCreateKeyboardEvent, CGEventPost, CGEventSetFlags};

#[cfg(target_os = "macos")]
pub(super) fn press_key(action: KeyAction<'_>) -> Result<Value> {
    const CG_HID_EVENT_TAP: u32 = 0;
    let key_code = key_code(action.key)?;
    let flags = action.modifiers.iter().fold(0_u64, |flags, modifier| {
        flags
            | match *modifier {
                "shift" => 1 << 17,
                "control" => 1 << 18,
                "option" => 1 << 19,
                "command" => 1 << 20,
                _ => 0,
            }
    });
    // SAFETY: CoreGraphics accepts a null source and returns retained keyboard events.
    let (down, up) = unsafe {
        (
            CGEventCreateKeyboardEvent(std::ptr::null(), key_code, true),
            CGEventCreateKeyboardEvent(std::ptr::null(), key_code, false),
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
        return Err(anyhow!(
            "macOS could not create the approved keyboard event"
        ));
    }
    // SAFETY: flags are set while both retained events are still owned by this scope.
    unsafe {
        CGEventSetFlags(down, flags);
        CGEventSetFlags(up, flags);
    }
    let key_up = CoreGraphicsUpGuard::new(up);
    // SAFETY: down is posted synchronously and released exactly once. The guard remains armed
    // across every later unwind path.
    unsafe {
        CGEventPost(CG_HID_EVENT_TAP, down);
        CFRelease(down);
    }
    key_up.release();
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "press_key",
        "key": action.key,
        "modifiers": action.modifiers,
    }))
}

#[cfg(target_os = "windows")]
pub(super) fn press_key(action: KeyAction<'_>) -> Result<Value> {
    super::windows::press_key(action)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn press_key(_action: KeyAction<'_>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}
