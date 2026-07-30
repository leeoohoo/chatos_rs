// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::path::Path;

use anyhow::{anyhow, Result};

#[cfg(target_os = "macos")]
use super::helper;
use super::{MACOS_OSASCRIPT_PATH, MACOS_SCREENCAPTURE_PATH};

#[cfg(target_os = "macos")]
#[repr(C)]
struct CFDictionaryKeyCallBacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
    hash: *const c_void,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct CFDictionaryValueCallBacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: *const c_void;
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> u8;
    fn CGRequestScreenCaptureAccess() -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: isize,
        key_callbacks: *const CFDictionaryKeyCallBacks,
        value_callbacks: *const CFDictionaryValueCallBacks,
    ) -> *const c_void;
    fn CFRelease(value: *const c_void);
}

#[cfg(target_os = "macos")]
pub(super) fn dependency_error() -> Option<String> {
    helper::dependency_error()
}

#[cfg(not(target_os = "macos"))]
pub(super) fn dependency_error() -> Option<String> {
    dependency_error_local()
}

pub(super) fn dependency_error_local() -> Option<String> {
    if cfg!(target_os = "windows") {
        return None;
    }
    if !cfg!(target_os = "macos") {
        return Some("Computer Use is unsupported on this platform".to_string());
    }
    if !Path::new(MACOS_OSASCRIPT_PATH).is_file() {
        return Some(format!(
            "macOS Automation runtime is missing: {MACOS_OSASCRIPT_PATH}"
        ));
    }
    if !macos_accessibility_is_trusted() {
        return Some(
            "macOS Accessibility permission is required for Computer Use observation".to_string(),
        );
    }
    screen_capture_dependency_error_local()
}

#[cfg(target_os = "macos")]
pub(super) fn screen_capture_dependency_error() -> Option<String> {
    helper::screen_capture_dependency_error()
}

#[cfg(not(target_os = "macos"))]
pub(super) fn screen_capture_dependency_error() -> Option<String> {
    screen_capture_dependency_error_local()
}

#[cfg(target_os = "macos")]
pub(super) fn request_permission(permission_id: &str) -> Result<bool> {
    helper::request_permission(permission_id)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn request_permission(_permission_id: &str) -> Result<bool> {
    Ok(false)
}

pub(super) fn screen_capture_dependency_error_local() -> Option<String> {
    if cfg!(target_os = "windows") {
        return None;
    }
    if !cfg!(target_os = "macos") {
        return Some("Computer Use screenshots are unsupported on this platform".to_string());
    }
    if !Path::new(MACOS_SCREENCAPTURE_PATH).is_file() {
        return Some(format!(
            "macOS screen capture runtime is missing: {MACOS_SCREENCAPTURE_PATH}"
        ));
    }
    (!macos_screen_capture_is_trusted()).then(|| {
        "macOS Screen Recording permission is required for Computer Use screenshots".to_string()
    })
}

#[cfg(target_os = "macos")]
fn macos_accessibility_is_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted takes no pointers, performs no prompt, and returns the current
    // process's TCC Accessibility trust state as a CoreServices Boolean.
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(target_os = "macos")]
fn macos_screen_capture_is_trusted() -> bool {
    // SAFETY: CGPreflightScreenCaptureAccess takes no pointers, does not request permission, and
    // returns only the current process's Screen Recording authorization state.
    unsafe { CGPreflightScreenCaptureAccess() != 0 }
}

#[cfg(target_os = "macos")]
pub(super) fn request_macos_accessibility_trust_prompt() -> bool {
    // SAFETY: The CoreFoundation dictionary is built from immutable process-wide constants and
    // released immediately after AXIsProcessTrustedWithOptions synchronously reads it.
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        if options.is_null() {
            return AXIsProcessTrusted() != 0;
        }
        let trusted = AXIsProcessTrustedWithOptions(options) != 0;
        CFRelease(options);
        trusted
    }
}

#[cfg(target_os = "macos")]
pub(super) fn request_macos_screen_capture_trust_prompt() -> bool {
    // SAFETY: CGRequestScreenCaptureAccess takes no pointers and asks macOS to prompt/register
    // Screen Recording access for the current helper process when possible.
    unsafe { CGRequestScreenCaptureAccess() != 0 }
}

#[cfg(not(target_os = "macos"))]
fn macos_screen_capture_is_trusted() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
fn macos_accessibility_is_trusted() -> bool {
    false
}

pub(super) fn ensure_observation_runtime() -> Result<()> {
    dependency_error_local()
        .map(|error| Err(anyhow!(error)))
        .unwrap_or(Ok(()))
}
