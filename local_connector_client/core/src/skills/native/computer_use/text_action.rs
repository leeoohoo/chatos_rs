// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(not(target_os = "windows"))]
use anyhow::anyhow;
use anyhow::Result;
use serde_json::{json, Value};

use super::action::TypedTextAction;
#[cfg(target_os = "macos")]
use super::input_guard::CoreGraphicsUpGuard;
#[cfg(target_os = "macos")]
use super::macos_text_target::ValidatedMacTextTarget;
#[cfg(target_os = "macos")]
use super::{CFRelease, CGEventCreateKeyboardEvent, CGEventKeyboardSetUnicodeString, CGEventPost};

#[cfg(target_os = "macos")]
pub(super) fn type_text(action: TypedTextAction<'_>) -> Result<Value> {
    const CG_HID_EVENT_TAP: u32 = 0;
    let target = ValidatedMacTextTarget::validate()?;
    target.ensure_still_focused()?;
    // SAFETY: CoreGraphics accepts a null source and returns retained keyboard events.
    let (down, up) = unsafe {
        (
            CGEventCreateKeyboardEvent(std::ptr::null(), 0, true),
            CGEventCreateKeyboardEvent(std::ptr::null(), 0, false),
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
        return Err(anyhow!("macOS could not create the approved text event"));
    }
    // SAFETY: CoreGraphics copies the bounded UTF-16 buffer before synchronous posting. The up
    // event is guarded first so failures never leave the generated key logically held.
    unsafe {
        CGEventKeyboardSetUnicodeString(down, action.utf16.len(), action.utf16.as_ptr());
    }
    let key_up = CoreGraphicsUpGuard::new(up);
    // SAFETY: down is posted synchronously and released exactly once.
    unsafe {
        CGEventPost(CG_HID_EVENT_TAP, down);
        CFRelease(down);
    }
    key_up.release();
    let mut result = typed_text_result(&action);
    let result_object = result
        .as_object_mut()
        .ok_or_else(|| anyhow!("Computer Use text result serialization failed"))?;
    result_object.insert("platform".to_string(), Value::String("macos".to_string()));
    result_object.insert(
        "target_class".to_string(),
        Value::String(target.class_name().to_string()),
    );
    Ok(result)
}

pub(super) fn typed_text_result(action: &TypedTextAction<'_>) -> Value {
    json!({
        "success": true,
        "mode": "approved_input",
        "action": "type_text",
        "character_count": action.character_count,
        "utf16_units": action.utf16.len(),
        "sha256": action.sha256,
        "text_persisted": false,
    })
}

#[cfg(target_os = "windows")]
pub(super) fn type_text(action: TypedTextAction<'_>) -> Result<Value> {
    super::windows::type_text(action)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn type_text(_action: TypedTextAction<'_>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use secure-field-aware text input is unsupported on this platform"
    ))
}
