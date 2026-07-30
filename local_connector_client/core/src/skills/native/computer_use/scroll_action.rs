// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(not(target_os = "windows"))]
use anyhow::anyhow;
use anyhow::Result;
use serde_json::{json, Value};

use super::action::ScrollAction;
#[cfg(target_os = "macos")]
use super::{CFRelease, CGEventCreateScrollWheelEvent2, CGEventPost};

#[cfg(target_os = "macos")]
pub(super) fn scroll(action: ScrollAction) -> Result<Value> {
    const CG_HID_EVENT_TAP: u32 = 0;
    const PIXEL_SCROLL: u32 = 0;
    // SAFETY: CoreGraphics returns one retained event from bounded integer deltas. The event is
    // posted synchronously and released exactly once.
    unsafe {
        let event = CGEventCreateScrollWheelEvent2(
            std::ptr::null(),
            PIXEL_SCROLL,
            2,
            action.delta_y,
            action.delta_x,
            0,
        );
        if event.is_null() {
            return Err(anyhow!("macOS could not create the approved scroll event"));
        }
        CGEventPost(CG_HID_EVENT_TAP, event);
        CFRelease(event);
    }
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "scroll",
        "delta_y": action.delta_y,
        "delta_x": action.delta_x,
    }))
}

#[cfg(target_os = "windows")]
pub(super) fn scroll(action: ScrollAction) -> Result<Value> {
    super::windows::scroll(action)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn scroll(_action: ScrollAction) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}
