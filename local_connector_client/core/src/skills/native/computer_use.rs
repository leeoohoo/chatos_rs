// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(target_os = "macos")]
mod helper;
#[cfg(target_os = "windows")]
mod windows;

use std::collections::BTreeMap;
#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::approval::{ApprovalActionAudit, ApprovalActionAuditDetail};

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
const COMPUTER_USE_SCREENSHOT_MAX_BYTES: usize = 2 * 1024 * 1024;
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

#[derive(Debug, Clone)]
struct DisplayTarget {
    index: u32,
    display_id: u32,
    is_main: bool,
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
    pixels_wide: usize,
    pixels_high: usize,
    rotation_degrees: f64,
}

#[derive(Debug, Clone)]
struct FrontmostWindowCaptureTarget {
    platform: &'static str,
    application: String,
    pid: u32,
    window_id: String,
    title: String,
    position: [f64; 2],
    size: [f64; 2],
    capture_position: [f64; 2],
    capture_size: [f64; 2],
    clipped_to_visible_desktop: bool,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Deserialize)]
struct MacosFrontmostWindowIdentity {
    application: String,
    pid: u32,
    window_id: u32,
    title: String,
    position: [f64; 2],
    size: [f64; 2],
}

#[cfg(target_os = "macos")]
impl MacosFrontmostWindowIdentity {
    fn validate(&self) -> Result<()> {
        if self.application.is_empty()
            || self.application.chars().count() > 240
            || self.pid == 0
            || self.window_id == 0
            || self.title.chars().count() > 500
            || self
                .position
                .iter()
                .chain(self.size.iter())
                .any(|value| !value.is_finite())
            || self.size[0] <= 0.0
            || self.size[1] <= 0.0
        {
            return Err(anyhow!(
                "macOS frontmost window identity or geometry is invalid"
            ));
        }
        Ok(())
    }

    fn same_identity_and_geometry(&self, other: &Self) -> bool {
        self.application == other.application
            && self.pid == other.pid
            && self.window_id == other.window_id
            && self.position == other.position
            && self.size == other.size
    }

    fn capture_target(&self) -> FrontmostWindowCaptureTarget {
        FrontmostWindowCaptureTarget {
            platform: "macos",
            application: self.application.clone(),
            pid: self.pid,
            window_id: self.window_id.to_string(),
            title: self.title.clone(),
            position: self.position,
            size: self.size,
            capture_position: self.position,
            capture_size: self.size,
            clipped_to_visible_desktop: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
struct ApprovedDisplayGuard {
    index: u32,
    display_id: u32,
    is_main: bool,
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
    pixels_wide: usize,
    pixels_high: usize,
    rotation_degrees: f64,
}

impl From<&DisplayTarget> for ApprovedDisplayGuard {
    fn from(display: &DisplayTarget) -> Self {
        Self {
            index: display.index,
            display_id: display.display_id,
            is_main: display.is_main,
            origin_x: display.origin_x,
            origin_y: display.origin_y,
            width: display.width,
            height: display.height,
            pixels_wide: display.pixels_wide,
            pixels_high: display.pixels_high,
            rotation_degrees: display.rotation_degrees,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ApprovedFrontmostWindowGuard {
    platform: String,
    application: String,
    pid: u32,
    window_id: String,
    position: [f64; 2],
    size: [f64; 2],
    fullscreen: Option<bool>,
    maximized: Option<bool>,
    position_settable: bool,
    size_settable: bool,
    fullscreen_settable: bool,
}

impl ApprovedFrontmostWindowGuard {
    fn validate(&self) -> Result<()> {
        if !matches!(self.platform.as_str(), "macos" | "windows")
            || self.application.is_empty()
            || self.application.chars().count() > 240
            || self.application.chars().any(is_unsafe_typed_character)
            || self.pid == 0
            || self.window_id.is_empty()
            || self.window_id.chars().count() > 64
            || self.window_id.chars().any(is_unsafe_typed_character)
            || self
                .position
                .iter()
                .chain(self.size.iter())
                .any(|value| !value.is_finite())
            || self.size[0] <= 0.0
            || self.size[1] <= 0.0
        {
            return Err(anyhow!(
                "frontmost window identity, state, or geometry is invalid"
            ));
        }
        match self.platform.as_str() {
            "macos" if self.fullscreen.is_none() || self.maximized.is_some() => {
                Err(anyhow!("macOS frontmost window state contract is invalid"))
            }
            "windows" if self.maximized.is_none() || self.fullscreen.is_some() => Err(anyhow!(
                "Windows frontmost window state contract is invalid"
            )),
            _ => Ok(()),
        }
    }

    fn geometry(&self) -> String {
        format!(
            "{} x {} @ {}, {}",
            format_audit_number(self.size[0]),
            format_audit_number(self.size[1]),
            format_audit_number(self.position[0]),
            format_audit_number(self.position[1]),
        )
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ApprovedWindowLayoutGuard {
    platform: String,
    application: String,
    process_identity: String,
    pid: u32,
    window_id: String,
    position: [f64; 2],
    size: [f64; 2],
}

impl ApprovedWindowLayoutGuard {
    fn validate(&self) -> Result<()> {
        if !matches!(self.platform.as_str(), "macos" | "windows")
            || self.application.is_empty()
            || self.application.chars().count() > 240
            || self.application.chars().any(is_unsafe_typed_character)
            || self.process_identity.is_empty()
            || self.process_identity.chars().count() > 500
            || self.process_identity.chars().any(is_unsafe_typed_character)
            || self.pid == 0
            || self.window_id.is_empty()
            || self.window_id.chars().count() > 64
            || self.window_id.chars().any(is_unsafe_typed_character)
            || self
                .position
                .iter()
                .chain(self.size.iter())
                .any(|value| !value.is_finite())
            || self.size[0] < MIN_WINDOW_DIMENSION as f64
            || self.size[1] < MIN_WINDOW_DIMENSION as f64
        {
            return Err(anyhow!("window layout identity or geometry is invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WindowLayoutCapturePayload {
    platform: String,
    #[serde(default)]
    display_layout: Vec<ApprovedDisplayGuard>,
    windows: Vec<ApprovedWindowLayoutGuard>,
    excluded_window_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WindowLayoutSnapshot {
    schema_version: u32,
    snapshot_id: String,
    snapshot_sha256: String,
    platform: String,
    display_layout: Vec<ApprovedDisplayGuard>,
    windows: Vec<ApprovedWindowLayoutGuard>,
    excluded_window_count: usize,
    truncated: bool,
}

impl WindowLayoutSnapshot {
    fn validate(&self) -> Result<()> {
        if self.schema_version != WINDOW_LAYOUT_SCHEMA_VERSION
            || !uuid::Uuid::parse_str(self.snapshot_id.as_str())
                .is_ok_and(|snapshot_id| snapshot_id.hyphenated().to_string() == self.snapshot_id)
            || self.snapshot_sha256.len() != 64
            || !self
                .snapshot_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self.platform != current_platform_name()
            || self.display_layout.is_empty()
            || self.display_layout.len() > MAX_ACTIVE_DISPLAYS
            || self.windows.is_empty()
            || self.windows.len() > MAX_WINDOW_LAYOUT_WINDOWS
            || self
                .windows
                .iter()
                .any(|window| window.platform != self.platform || window.validate().is_err())
            || window_layout_sha256(self)? != self.snapshot_sha256
        {
            return Err(anyhow!("window layout snapshot is invalid"));
        }
        for window in &self.windows {
            validate_window_layout_geometry(window, &self.display_layout)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct StoredWindowLayoutSnapshot {
    captured_at: Instant,
    snapshot: WindowLayoutSnapshot,
}

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
    fn AXIsProcessTrusted() -> u8;
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
    fn CGPreflightScreenCaptureAccess() -> u8;
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

const LIST_WINDOWS_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function pair(value) {
  try {
    return [Number(value[0]), Number(value[1])];
  } catch (_) {
    return null;
  }
}
function run(argv) {
  var limit = Number.parseInt(argv[0] || "40", 10);
  if (!Number.isFinite(limit) || limit < 1) limit = 40;
  limit = Math.min(limit, 100);
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({backgroundOnly: false})();
  var rows = [];
  for (var processIndex = 0; processIndex < processes.length && rows.length < limit; processIndex += 1) {
    var process = processes[processIndex];
    var frontmost = false;
    var processName = "";
    var processId = null;
    try { frontmost = Boolean(process.frontmost()); } catch (_) {}
    try { processName = text(process.name(), 240); } catch (_) {}
    try { processId = Number(process.unixId()); } catch (_) {}
    var windows = [];
    try {
      var processWindows = process.windows();
      for (var windowIndex = 0; windowIndex < processWindows.length && windows.length < 20; windowIndex += 1) {
        var window = processWindows[windowIndex];
        var title = "";
        var position = null;
        var size = null;
        try { title = text(window.name(), 500); } catch (_) {}
        try { position = pair(window.position()); } catch (_) {}
        try { size = pair(window.size()); } catch (_) {}
        windows.push({title: title, position: position, size: size});
      }
    } catch (_) {}
    if (frontmost || windows.length > 0) {
      rows.push({name: processName, pid: processId, frontmost: frontmost, windows: windows});
    }
  }
  rows.sort(function(left, right) {
    if (left.frontmost === right.frontmost) return left.name.localeCompare(right.name);
    return left.frontmost ? -1 : 1;
  });
  return JSON.stringify({platform: "macos", process_count: rows.length, processes: rows});
}
"#;

const CAPTURE_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function pair(value) {
  try {
    var first = Number(value[0]); var second = Number(value[1]);
    return Number.isFinite(first) && Number.isFinite(second) ? [first, second] : null;
  } catch (_) { return null; }
}
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) {
  var candidate = attribute(window, name);
  return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback);
}
function attributeSettable(window, name) {
  var candidate = attribute(window, name);
  return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false));
}
function processIdentity(process) {
  var bundleIdentifier = text(safe(function() { return process.bundleIdentifier(); }, ""), 480);
  return bundleIdentifier ? "bundle:" + bundleIdentifier : "";
}
function run(argv) {
  var maximum = Number.parseInt(argv[0] || "8", 10);
  if (!Number.isFinite(maximum) || maximum < 1 || maximum > 8) throw new Error("Window layout limit is invalid");
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({backgroundOnly: false})();
  var rows = []; var excluded = 0; var truncated = false;
  for (var processIndex = 0; processIndex < processes.length && !truncated; processIndex += 1) {
    var process = processes[processIndex];
    var application = text(safe(function() { return process.name(); }, ""), 240);
    var pid = Number(safe(function() { return process.unixId(); }, 0));
    var identity = processIdentity(process);
    var windows = safe(function() { return process.windows(); }, []);
    for (var windowIndex = 0; windowIndex < windows.length; windowIndex += 1) {
      var window = windows[windowIndex];
      var windowId = Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)));
      var position = pair(safe(function() { return window.position(); }, null));
      var size = pair(safe(function() { return window.size(); }, null));
      var subrole = text(safe(function() { return window.subrole(); }, ""), 120);
      var visible = Boolean(safe(function() { return window.visible(); }, false));
      var minimized = Boolean(attributeValue(window, "AXMinimized", false));
      var fullscreen = Boolean(attributeValue(window, "AXFullScreen", false));
      var eligible = application && identity && Number.isFinite(pid) && pid >= 1 && Math.floor(pid) === pid &&
        Number.isFinite(windowId) && windowId >= 1 && Math.floor(windowId) === windowId &&
        subrole === "AXStandardWindow" && visible && !minimized && !fullscreen &&
        position !== null && size !== null && size[0] >= 64 && size[1] >= 64 &&
        attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
      if (!eligible) { excluded += 1; continue; }
      if (rows.length >= maximum) { truncated = true; break; }
      rows.push({
        platform: "macos", application: application, process_identity: identity, pid: pid,
        window_id: String(windowId), position: position, size: size
      });
    }
  }
  if (rows.length === 0) throw new Error("No ordinary restorable macOS windows are available");
  return JSON.stringify({platform: "macos", windows: rows, excluded_window_count: excluded, truncated: truncated});
}
"#;

const PREFLIGHT_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) {
  try { var a = Number(value[0]); var b = Number(value[1]); return Number.isFinite(a) && Number.isFinite(b) ? [a, b] : null; }
  catch (_) { return null; }
}
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) { var candidate = attribute(window, name); return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback); }
function attributeSettable(window, name) { var candidate = attribute(window, name); return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false)); }
function processIdentity(process) { var value = String(safe(function() { return process.bundleIdentifier(); }, "")); return value ? "bundle:" + value : ""; }
function currentWindow(systemEvents, guard) {
  var processes = systemEvents.applicationProcesses(); var process = null;
  for (var p = 0; p < processes.length; p += 1) {
    var candidate = processes[p];
    if (Number(safe(function() { return candidate.unixId(); }, 0)) === guard.pid &&
        String(safe(function() { return candidate.name(); }, "")) === guard.application &&
        processIdentity(candidate) === guard.process_identity) { process = candidate; break; }
  }
  if (process === null) return null;
  var windows = safe(function() { return process.windows(); }, []);
  for (var w = 0; w < windows.length; w += 1) {
    var window = windows[w];
    var id = String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0))));
    if (id !== guard.window_id) continue;
    var position = pair(safe(function() { return window.position(); }, null));
    var size = pair(safe(function() { return window.size(); }, null));
    var ordinary = String(safe(function() { return window.subrole(); }, "")) === "AXStandardWindow" &&
      Boolean(safe(function() { return window.visible(); }, false)) && !Boolean(attributeValue(window, "AXMinimized", false)) &&
      !Boolean(attributeValue(window, "AXFullScreen", false)) && attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
    return ordinary && position !== null && size !== null && size[0] >= 64 && size[1] >= 64 ? {position: position, size: size} : null;
  }
  return null;
}
function run(argv) {
  var snapshot = JSON.parse(argv[0]); var systemEvents = Application("System Events");
  for (var index = 0; index < snapshot.windows.length; index += 1) {
    if (currentWindow(systemEvents, snapshot.windows[index]) === null) {
      throw new Error("A snapshotted macOS window identity, capability, or ordinary-window state changed before approval");
    }
  }
  return JSON.stringify({validated: true, window_count: snapshot.windows.length});
}
"#;

const RESTORE_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) { try { var a = Number(value[0]); var b = Number(value[1]); return Number.isFinite(a) && Number.isFinite(b) ? [a, b] : null; } catch (_) { return null; } }
function equalPair(left, right) { return left !== null && right !== null && left[0] === right[0] && left[1] === right[1]; }
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) { var candidate = attribute(window, name); return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback); }
function attributeSettable(window, name) { var candidate = attribute(window, name); return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false)); }
function processIdentity(process) { var value = String(safe(function() { return process.bundleIdentifier(); }, "")); return value ? "bundle:" + value : ""; }
function currentWindow(systemEvents, guard) {
  var processes = systemEvents.applicationProcesses(); var process = null;
  for (var p = 0; p < processes.length; p += 1) {
    var candidate = processes[p];
    if (Number(safe(function() { return candidate.unixId(); }, 0)) === guard.pid && String(safe(function() { return candidate.name(); }, "")) === guard.application && processIdentity(candidate) === guard.process_identity) { process = candidate; break; }
  }
  if (process === null) return null;
  var windows = safe(function() { return process.windows(); }, []);
  for (var w = 0; w < windows.length; w += 1) {
    var window = windows[w]; var id = String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0))));
    if (id !== guard.window_id) continue;
    var position = pair(safe(function() { return window.position(); }, null)); var size = pair(safe(function() { return window.size(); }, null));
    var ordinary = String(safe(function() { return window.subrole(); }, "")) === "AXStandardWindow" && Boolean(safe(function() { return window.visible(); }, false)) &&
      !Boolean(attributeValue(window, "AXMinimized", false)) && !Boolean(attributeValue(window, "AXFullScreen", false)) &&
      attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
    return ordinary && position !== null && size !== null && size[0] >= 64 && size[1] >= 64 ? {window: window, position: position, size: size} : null;
  }
  return null;
}
function rollback(systemEvents, snapshot, before, applied) {
  var restored = 0; var skipped = 0; var failed = 0;
  for (var offset = applied.length - 1; offset >= 0; offset -= 1) {
    var index = applied[offset]; var current = currentWindow(systemEvents, snapshot.windows[index]);
    if (current === null || !equalPair(current.position, snapshot.windows[index].position) || !equalPair(current.size, snapshot.windows[index].size)) { skipped += 1; continue; }
    try { current.window.size.set(before[index].size); current.window.position.set(before[index].position); }
    catch (_) { failed += 1; continue; }
    var after = currentWindow(systemEvents, snapshot.windows[index]);
    if (after !== null && equalPair(after.position, before[index].position) && equalPair(after.size, before[index].size)) restored += 1; else failed += 1;
  }
  return {attempted: applied.length > 0, restored_count: restored, skipped_count: skipped, failed_count: failed, complete: restored === applied.length};
}
function failure(reason, systemEvents, snapshot, before, applied, partialIndex) {
  var recovery = rollback(systemEvents, snapshot, before, applied);
  return JSON.stringify({
    success: false, mode: "approved_input", action: "restore_window_layout", platform: "macos",
    snapshot_id: snapshot.snapshot_id, snapshot_sha256: snapshot.snapshot_sha256,
    target_window_count: snapshot.windows.length, applied_window_count: applied.length,
    target_layout_retained: false,
    action_already_executed: applied.length > 0 || partialIndex !== null, automatic_replay_safe: false,
    failure_reason: reason, partial_window_index: partialIndex,
    window_layout_recovery: recovery,
    application_content_rollback: false, manual_review_required: partialIndex !== null || !recovery.complete
  });
}
function run(argv) {
  var snapshot = JSON.parse(argv[0]); var systemEvents = Application("System Events"); var before = [];
  for (var index = 0; index < snapshot.windows.length; index += 1) {
    var current = currentWindow(systemEvents, snapshot.windows[index]);
    if (current === null) throw new Error("A snapshotted macOS window identity, capability, or ordinary-window state changed before layout restore");
    before.push({platform: "macos", application: snapshot.windows[index].application, process_identity: snapshot.windows[index].process_identity,
      pid: snapshot.windows[index].pid, window_id: snapshot.windows[index].window_id, position: current.position, size: current.size});
  }
  var applied = [];
  for (var targetIndex = 0; targetIndex < snapshot.windows.length; targetIndex += 1) {
    var target = snapshot.windows[targetIndex]; var live = currentWindow(systemEvents, target);
    if (live === null || !equalPair(live.position, before[targetIndex].position) || !equalPair(live.size, before[targetIndex].size)) {
      return failure("window_drift_during_restore", systemEvents, snapshot, before, applied, null);
    }
    try { live.window.size.set(target.size); live.window.position.set(target.position); }
    catch (_) { return failure("platform_apply_failed", systemEvents, snapshot, before, applied, targetIndex); }
    var after = currentWindow(systemEvents, target);
    if (after === null || !equalPair(after.position, target.position) || !equalPair(after.size, target.size)) {
      return failure("target_geometry_readback_mismatch", systemEvents, snapshot, before, applied, targetIndex);
    }
    applied.push(targetIndex);
  }
  delay(0.16);
  for (var verifyIndex = 0; verifyIndex < snapshot.windows.length; verifyIndex += 1) {
    var verified = currentWindow(systemEvents, snapshot.windows[verifyIndex]);
    if (verified === null || !equalPair(verified.position, snapshot.windows[verifyIndex].position) || !equalPair(verified.size, snapshot.windows[verifyIndex].size)) {
      return failure("post_action_window_drift", systemEvents, snapshot, before, applied, null);
    }
  }
  return JSON.stringify({
    success: true, mode: "approved_input", action: "restore_window_layout", platform: "macos",
    snapshot_id: snapshot.snapshot_id, snapshot_sha256: snapshot.snapshot_sha256,
    target_window_count: snapshot.windows.length, restored_window_count: snapshot.windows.length,
    identity_geometry_and_display_layout_revalidated: true, automatic_replay_safe: false,
    application_content_rollback: false, pre_action_windows: before,
    window_layout_recovery: {attempted: false, restored_count: 0, skipped_count: 0, failed_count: 0, complete: false}
  });
}
"#;

const ROLLBACK_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) { try { var a = Number(value[0]); var b = Number(value[1]); return Number.isFinite(a) && Number.isFinite(b) ? [a, b] : null; } catch (_) { return null; } }
function equalPair(left, right) { return left !== null && right !== null && left[0] === right[0] && left[1] === right[1]; }
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) { var candidate = attribute(window, name); return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback); }
function attributeSettable(window, name) { var candidate = attribute(window, name); return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false)); }
function processIdentity(process) { var value = String(safe(function() { return process.bundleIdentifier(); }, "")); return value ? "bundle:" + value : ""; }
function currentWindow(systemEvents, guard) {
  var processes = systemEvents.applicationProcesses();
  for (var p = 0; p < processes.length; p += 1) {
    var process = processes[p];
    if (Number(safe(function() { return process.unixId(); }, 0)) !== guard.pid || String(safe(function() { return process.name(); }, "")) !== guard.application || processIdentity(process) !== guard.process_identity) continue;
    var windows = safe(function() { return process.windows(); }, []);
    for (var w = 0; w < windows.length; w += 1) {
      var window = windows[w]; var id = String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0))));
      if (id !== guard.window_id) continue;
      var ordinary = String(safe(function() { return window.subrole(); }, "")) === "AXStandardWindow" && Boolean(safe(function() { return window.visible(); }, false)) &&
        !Boolean(attributeValue(window, "AXMinimized", false)) && !Boolean(attributeValue(window, "AXFullScreen", false)) &&
        attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
      if (!ordinary) return null;
      return {window: window, position: pair(safe(function() { return window.position(); }, null)), size: pair(safe(function() { return window.size(); }, null))};
    }
  }
  return null;
}
function run(argv) {
  var snapshot = JSON.parse(argv[0]); var before = JSON.parse(argv[1]); var systemEvents = Application("System Events");
  var restored = 0; var skipped = 0; var failed = 0;
  for (var index = snapshot.windows.length - 1; index >= 0; index -= 1) {
    var current = currentWindow(systemEvents, snapshot.windows[index]);
    if (current === null || !equalPair(current.position, snapshot.windows[index].position) || !equalPair(current.size, snapshot.windows[index].size)) { skipped += 1; continue; }
    try { current.window.size.set(before[index].size); current.window.position.set(before[index].position); }
    catch (_) { failed += 1; continue; }
    var after = currentWindow(systemEvents, snapshot.windows[index]);
    if (after !== null && equalPair(after.position, before[index].position) && equalPair(after.size, before[index].size)) restored += 1; else failed += 1;
  }
  return JSON.stringify({attempted: true, restored_count: restored, skipped_count: skipped, failed_count: failed, complete: restored === snapshot.windows.length});
}
"#;

const INSPECT_FRONTMOST_WINDOW_JXA: &str = r#"
function safe(callable, fallback) {
  try { return callable(); } catch (_) { return fallback; }
}
function attributeValue(element, name, fallback) {
  return safe(function() { return element.attributes.byName(name).value(); }, fallback);
}
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function editableValue(element, role) {
  if (role === "AXTextField" || role === "AXTextArea" || role === "AXComboBox" || role === "AXSearchField") {
    return true;
  }
  if (attributeValue(element, "AXIsEditable", false) === true) return true;
  return attributeValue(element, "AXEditableAncestor", null) !== null ||
    attributeValue(element, "AXHighestEditableAncestor", null) !== null;
}
function visibleValueAllowed(role, subrole) {
  var normalized = (String(role || "") + " " + String(subrole || "")).toLowerCase();
  if (normalized.indexOf("secure") >= 0 || normalized.indexOf("password") >= 0) return false;
  return role === "AXStaticText" || role === "AXButton" || role === "AXCheckBox" ||
    role === "AXRadioButton" || role === "AXMenuItem" || role === "AXPopUpButton" ||
    role === "AXSlider" || role === "AXProgressIndicator";
}
function run(argv) {
  var maxDepth = Number.parseInt(argv[0] || "4", 10);
  var maxNodes = Number.parseInt(argv[1] || "200", 10);
  if (!Number.isFinite(maxDepth) || maxDepth < 1) maxDepth = 4;
  if (!Number.isFinite(maxNodes) || maxNodes < 1) maxNodes = 200;
  maxDepth = Math.min(maxDepth, 6);
  maxNodes = Math.min(maxNodes, 400);
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length === 0) throw new Error("No frontmost application process is available");
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) throw new Error("The frontmost application has no inspectable window");
  var nodes = [];
  function visit(element, depth) {
    if (nodes.length >= maxNodes) return null;
    var role = text(safe(function() { return element.role(); }, ""), 120);
    var subrole = text(safe(function() { return element.subrole(); }, ""), 120);
    var editable = editableValue(element, role);
    var node = {
      ref: "u" + String(nodes.length + 1),
      role: role,
      subrole: subrole,
      name: text(safe(function() { return element.name(); }, ""), 500),
      description: text(safe(function() { return element.description(); }, ""), 500),
      enabled: Boolean(safe(function() { return element.enabled(); }, true)),
      children: []
    };
    if (editable) {
      node.editable = true;
      node.value_redacted = true;
    } else if (visibleValueAllowed(role, subrole)) {
      node.value = text(safe(function() { return element.value(); }, ""), 500);
    }
    nodes.push(node);
    if (depth < maxDepth && nodes.length < maxNodes) {
      var children = safe(function() { return element.uiElements(); }, []);
      for (var childIndex = 0; childIndex < children.length && nodes.length < maxNodes; childIndex += 1) {
        var child = visit(children[childIndex], depth + 1);
        if (child !== null) node.children.push(child);
      }
    }
    return node;
  }
  var tree = visit(windows[0], 0);
  return JSON.stringify({
    platform: "macos",
    application: text(safe(function() { return process.name(); }, ""), 240),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_title: text(safe(function() { return windows[0].name(); }, ""), 500),
    node_count: nodes.length,
    truncated: nodes.length >= maxNodes,
    text_entry_values_redacted: true,
    tree: tree
  });
}
"#;

const FRONTMOST_WINDOW_CAPTURE_TARGET_JXA: &str = r#"
function safe(callable, fallback) {
  try { return callable(); } catch (_) { return fallback; }
}
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function pair(value) {
  try {
    var first = Number(value[0]);
    var second = Number(value[1]);
    if (!Number.isFinite(first) || !Number.isFinite(second)) return null;
    return [first, second];
  } catch (_) {
    return null;
  }
}
function attributeValue(element, name, fallback) {
  return safe(function() { return element.attributes.byName(name).value(); }, fallback);
}
function run() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length === 0) throw new Error("No frontmost application process is available");
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) throw new Error("The frontmost application has no capturable window");
  var window = windows[0];
  var visible = Boolean(safe(function() { return window.visible(); }, true));
  var minimized = Boolean(attributeValue(window, "AXMinimized", false));
  var windowId = Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)));
  var pid = Number(safe(function() { return process.unixId(); }, 0));
  var position = pair(safe(function() { return window.position(); }, null));
  var size = pair(safe(function() { return window.size(); }, null));
  if (!visible || minimized) throw new Error("The frontmost window is not visibly capturable");
  if (!Number.isFinite(windowId) || windowId < 1 || Math.floor(windowId) !== windowId) {
    throw new Error("The frontmost window identity is invalid");
  }
  if (!Number.isFinite(pid) || pid < 1 || Math.floor(pid) !== pid) {
    throw new Error("The frontmost application identity is invalid");
  }
  if (position === null || size === null || size[0] <= 0 || size[1] <= 0) {
    throw new Error("The frontmost window geometry is invalid");
  }
  if (!Boolean(process.frontmost())) throw new Error("The frontmost application changed during observation");
  return JSON.stringify({
    platform: "macos",
    application: text(safe(function() { return process.name(); }, ""), 240),
    pid: pid,
    window_id: windowId,
    position: position,
    size: size
  });
}
"#;

const FRONTMOST_WINDOW_CONTROL_TARGET_JXA: &str = r#"
function safe(callable, fallback) {
  try { return callable(); } catch (_) { return fallback; }
}
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function pair(value) {
  try {
    var first = Number(value[0]);
    var second = Number(value[1]);
    if (!Number.isFinite(first) || !Number.isFinite(second)) return null;
    return [first, second];
  } catch (_) {
    return null;
  }
}
function attribute(window, name) {
  return safe(function() { return window.attributes.byName(name); }, null);
}
function attributeValue(window, name, fallback) {
  var candidate = attribute(window, name);
  return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback);
}
function attributeSettable(window, name) {
  var candidate = attribute(window, name);
  return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false));
}
function run() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) throw new Error("A unique frontmost application process is required");
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) throw new Error("The frontmost application has no controllable window");
  var window = windows[0];
  var visible = Boolean(safe(function() { return window.visible(); }, false));
  var minimized = Boolean(attributeValue(window, "AXMinimized", false));
  var windowId = Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)));
  var pid = Number(safe(function() { return process.unixId(); }, 0));
  var position = pair(safe(function() { return window.position(); }, null));
  var size = pair(safe(function() { return window.size(); }, null));
  var fullscreenAttribute = attribute(window, "AXFullScreen");
  var fullscreen = fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false));
  if (!visible || minimized) throw new Error("The frontmost window is not visibly controllable");
  if (!Number.isFinite(windowId) || windowId < 1 || Math.floor(windowId) !== windowId) {
    throw new Error("The frontmost window identity is invalid");
  }
  if (!Number.isFinite(pid) || pid < 1 || Math.floor(pid) !== pid) {
    throw new Error("The frontmost application identity is invalid");
  }
  if (position === null || size === null || size[0] <= 0 || size[1] <= 0) {
    throw new Error("The frontmost window geometry is invalid");
  }
  if (!Boolean(process.frontmost())) throw new Error("The frontmost application changed during observation");
  return JSON.stringify({
    platform: "macos",
    application: text(safe(function() { return process.name(); }, ""), 240),
    pid: pid,
    window_id: String(windowId),
    title: text(safe(function() { return window.name(); }, ""), 500),
    position: position,
    size: size,
    fullscreen: fullscreen,
    maximized: null,
    position_settable: attributeSettable(window, "AXPosition"),
    size_settable: attributeSettable(window, "AXSize"),
    fullscreen_settable: fullscreenAttribute !== null && attributeSettable(window, "AXFullScreen")
  });
}
"#;

const SET_FRONTMOST_WINDOW_BOUNDS_JXA: &str = r#"
function safe(callable, fallback) {
  try { return callable(); } catch (_) { return fallback; }
}
function pair(value) {
  try {
    var first = Number(value[0]);
    var second = Number(value[1]);
    if (!Number.isFinite(first) || !Number.isFinite(second)) return null;
    return [first, second];
  } catch (_) { return null; }
}
function attribute(window, name) {
  return safe(function() { return window.attributes.byName(name); }, null);
}
function attributeValue(window, name, fallback) {
  var candidate = attribute(window, name);
  return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback);
}
function attributeSettable(window, name) {
  var candidate = attribute(window, name);
  return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false));
}
function currentTarget() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) return null;
  var window = windows[0];
  var position = pair(safe(function() { return window.position(); }, null));
  var size = pair(safe(function() { return window.size(); }, null));
  var fullscreenAttribute = attribute(window, "AXFullScreen");
  if (position === null || size === null) return null;
  return {
    process: process,
    window: window,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    position: position,
    size: size,
    fullscreen: fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false)),
    position_settable: attributeSettable(window, "AXPosition"),
    size_settable: attributeSettable(window, "AXSize"),
    fullscreen_settable: fullscreenAttribute !== null && attributeSettable(window, "AXFullScreen"),
    visible: Boolean(safe(function() { return window.visible(); }, false)),
    minimized: Boolean(attributeValue(window, "AXMinimized", false)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function equalPair(left, right) {
  return left !== null && right !== null && left[0] === right[0] && left[1] === right[1];
}
function matchesApproved(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid &&
    target.window_id === approved.window_id && equalPair(target.position, approved.position) &&
    equalPair(target.size, approved.size) && target.fullscreen === approved.fullscreen &&
    target.position_settable === approved.position_settable &&
    target.size_settable === approved.size_settable &&
    target.fullscreen_settable === approved.fullscreen_settable;
}
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid &&
    target.window_id === approved.window_id;
}
function recoveryResult(approved) {
  var current = currentTarget();
  if (!identityMatches(current, approved)) {
    return {attempted: false, restored: false, reason: "foreground_or_identity_changed"};
  }
  try {
    current.window.size.set(approved.size);
    current.window.position.set(approved.position);
  } catch (_) {
    return {attempted: true, restored: false, reason: "platform_restore_failed"};
  }
  var restored = currentTarget();
  var exact = matchesApproved(restored, approved);
  return {attempted: true, restored: exact, reason: exact ? "original_geometry_restored" : "restore_readback_mismatch"};
}
function run(argv) {
  var approved = JSON.parse(argv[0]);
  var requested = JSON.parse(argv[1]);
  var before = currentTarget();
  if (!matchesApproved(before, approved)) {
    throw new Error("The approved frontmost window identity, state, capability, or geometry changed before bounds control");
  }
  if (before.fullscreen || !before.position_settable || !before.size_settable) {
    throw new Error("The approved frontmost window is not safely movable and resizable");
  }
  try {
    before.window.size.set([requested.width, requested.height]);
    before.window.position.set([requested.x, requested.y]);
  } catch (_) {
    return JSON.stringify({
      success: false,
      mode: "approved_input",
      action: "set_frontmost_window_bounds",
      target_geometry_applied: false,
      action_already_executed: true,
      automatic_replay_safe: false,
      failure_reason: "platform_apply_failed",
      window_geometry_recovery: recoveryResult(approved)
    });
  }
  var after = currentTarget();
  var exact = identityMatches(after, approved) &&
    equalPair(after.position, [requested.x, requested.y]) &&
    equalPair(after.size, [requested.width, requested.height]) && !after.fullscreen;
  if (!exact) {
    return JSON.stringify({
      success: false,
      mode: "approved_input",
      action: "set_frontmost_window_bounds",
      target_geometry_applied: false,
      action_already_executed: true,
      automatic_replay_safe: false,
      failure_reason: "target_geometry_readback_mismatch",
      window_geometry_recovery: recoveryResult(approved)
    });
  }
  return JSON.stringify({
    success: true,
    mode: "approved_input",
    action: "set_frontmost_window_bounds",
    platform: "macos",
    application: approved.application,
    pid: approved.pid,
    window_id: approved.window_id,
    original_position: approved.position,
    original_size: approved.size,
    position: after.position,
    size: after.size,
    target_geometry_applied: true,
    identity_and_geometry_revalidated_after_action: true,
    window_geometry_recovery: {attempted: false, restored: false, reason: "action_completed"}
  });
}
"#;

const RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) {
  try {
    var first = Number(value[0]); var second = Number(value[1]);
    return Number.isFinite(first) && Number.isFinite(second) ? [first, second] : null;
  } catch (_) { return null; }
}
function attributeValue(window, name, fallback) {
  return safe(function() { return window.attributes.byName(name).value(); }, fallback);
}
function currentTarget() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0]; var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) return null;
  var window = windows[0];
  return {
    process: process, window: window,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    position: pair(safe(function() { return window.position(); }, null)),
    size: pair(safe(function() { return window.size(); }, null)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function equalPair(left, right) { return left !== null && left[0] === right[0] && left[1] === right[1]; }
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.application === approved.application &&
    target.pid === approved.pid && target.window_id === approved.window_id;
}
function run(argv) {
  var approved = JSON.parse(argv[0]); var requested = JSON.parse(argv[1]);
  var current = currentTarget();
  if (!identityMatches(current, approved) || !equalPair(current.position, [requested.x, requested.y]) ||
      !equalPair(current.size, [requested.width, requested.height])) {
    return JSON.stringify({attempted: false, restored: false, reason: "foreground_identity_or_target_geometry_changed"});
  }
  try { current.window.size.set(approved.size); current.window.position.set(approved.position); }
  catch (_) { return JSON.stringify({attempted: true, restored: false, reason: "platform_restore_failed"}); }
  var after = currentTarget();
  var restored = identityMatches(after, approved) && equalPair(after.position, approved.position) && equalPair(after.size, approved.size);
  return JSON.stringify({attempted: true, restored: restored, reason: restored ? "cancelled_action_restored" : "restore_readback_mismatch"});
}
"#;

const SET_FRONTMOST_WINDOW_FULLSCREEN_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) {
  try {
    var first = Number(value[0]); var second = Number(value[1]);
    return Number.isFinite(first) && Number.isFinite(second) ? [first, second] : null;
  } catch (_) { return null; }
}
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) {
  var candidate = attribute(window, name);
  return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback);
}
function attributeSettable(window, name) {
  var candidate = attribute(window, name);
  return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false));
}
function currentTarget() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0]; var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) return null;
  var window = windows[0]; var fullscreenAttribute = attribute(window, "AXFullScreen");
  return {
    process: process, window: window, fullscreen_attribute: fullscreenAttribute,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    position: pair(safe(function() { return window.position(); }, null)),
    size: pair(safe(function() { return window.size(); }, null)),
    fullscreen: fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false)),
    position_settable: attributeSettable(window, "AXPosition"),
    size_settable: attributeSettable(window, "AXSize"),
    fullscreen_settable: fullscreenAttribute !== null && attributeSettable(window, "AXFullScreen"),
    visible: Boolean(safe(function() { return window.visible(); }, false)),
    minimized: Boolean(attributeValue(window, "AXMinimized", false)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function equalPair(left, right) { return left !== null && left[0] === right[0] && left[1] === right[1]; }
function matchesApproved(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid &&
    target.window_id === approved.window_id && equalPair(target.position, approved.position) &&
    equalPair(target.size, approved.size) && target.fullscreen === approved.fullscreen &&
    target.position_settable === approved.position_settable && target.size_settable === approved.size_settable &&
    target.fullscreen_settable === approved.fullscreen_settable;
}
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid && target.window_id === approved.window_id;
}
function waitForState(approved, expected) {
  for (var index = 0; index < 20; index += 1) {
    var current = currentTarget();
    if (!identityMatches(current, approved)) return current;
    if (current.fullscreen === expected) return current;
    delay(0.04);
  }
  return currentTarget();
}
function restoreState(approved) {
  var current = currentTarget();
  if (!identityMatches(current, approved) || current.fullscreen_attribute === null || !current.fullscreen_settable) {
    return {attempted: false, restored: false, reason: "foreground_identity_or_capability_changed"};
  }
  try { current.fullscreen_attribute.value.set(approved.fullscreen); }
  catch (_) { return {attempted: true, restored: false, reason: "platform_restore_failed"}; }
  var restored = waitForState(approved, approved.fullscreen);
  var exact = identityMatches(restored, approved) && restored.fullscreen === approved.fullscreen;
  return {attempted: true, restored: exact, reason: exact ? "original_fullscreen_state_restored" : "restore_readback_mismatch"};
}
function run(argv) {
  var approved = JSON.parse(argv[0]); var requested = argv[1] === "true";
  var before = currentTarget();
  if (!matchesApproved(before, approved)) {
    throw new Error("The approved frontmost window identity, state, capability, or geometry changed before fullscreen control");
  }
  if (before.fullscreen_attribute === null || !before.fullscreen_settable || before.fullscreen === requested) {
    throw new Error("The approved frontmost window fullscreen transition is unavailable");
  }
  try { before.fullscreen_attribute.value.set(requested); }
  catch (_) {
    return JSON.stringify({
      success: false, mode: "approved_input", action: "set_frontmost_window_fullscreen",
      target_fullscreen_applied: false, action_already_executed: true, automatic_replay_safe: false,
      failure_reason: "platform_apply_failed", window_state_recovery: restoreState(approved)
    });
  }
  var after = waitForState(approved, requested);
  if (!identityMatches(after, approved) || after.fullscreen !== requested) {
    return JSON.stringify({
      success: false, mode: "approved_input", action: "set_frontmost_window_fullscreen",
      target_fullscreen_applied: false, action_already_executed: true, automatic_replay_safe: false,
      failure_reason: "target_state_readback_mismatch", window_state_recovery: restoreState(approved)
    });
  }
  return JSON.stringify({
    success: true, mode: "approved_input", action: "set_frontmost_window_fullscreen", platform: "macos",
    application: approved.application, pid: approved.pid, window_id: approved.window_id,
    original_fullscreen: approved.fullscreen, fullscreen: after.fullscreen,
    position: after.position, size: after.size, target_fullscreen_applied: true,
    identity_and_state_revalidated_after_action: true,
    window_state_recovery: {attempted: false, restored: false, reason: "action_completed"}
  });
}
"#;

const RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function attributeValue(window, name, fallback) { return safe(function() { return window.attributes.byName(name).value(); }, fallback); }
function currentTarget() {
  var systemEvents = Application("System Events"); var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0]; var windows = safe(function() { return process.windows(); }, []); if (windows.length === 0) return null;
  var window = windows[0]; var fullscreenAttribute = safe(function() { return window.attributes.byName("AXFullScreen"); }, null);
  return {
    window: window, fullscreen_attribute: fullscreenAttribute,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    fullscreen: fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.application === approved.application &&
    target.pid === approved.pid && target.window_id === approved.window_id;
}
function run(argv) {
  var approved = JSON.parse(argv[0]); var requested = argv[1] === "true"; var current = currentTarget();
  if (!identityMatches(current, approved) || current.fullscreen !== requested || current.fullscreen_attribute === null) {
    return JSON.stringify({attempted: false, restored: false, reason: "foreground_identity_or_target_state_changed"});
  }
  try { current.fullscreen_attribute.value.set(approved.fullscreen); }
  catch (_) { return JSON.stringify({attempted: true, restored: false, reason: "platform_restore_failed"}); }
  for (var index = 0; index < 20; index += 1) {
    var after = currentTarget();
    if (!identityMatches(after, approved) || after.fullscreen === approved.fullscreen) break;
    delay(0.04);
  }
  var restored = currentTarget(); var exact = identityMatches(restored, approved) && restored.fullscreen === approved.fullscreen;
  return JSON.stringify({attempted: true, restored: exact, reason: exact ? "cancelled_action_restored" : "restore_readback_mismatch"});
}
"#;

const LOOKUP_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function processForPid(systemEvents, pid) {
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Number(candidate.unixId()) === pid) return candidate;
    } catch (_) {}
  }
  return null;
}
function run(argv) {
  var pid = Number.parseInt(argv[0] || "0", 10);
  if (!Number.isFinite(pid) || pid < 1) throw new Error("A positive application PID is required");
  var systemEvents = Application("System Events");
  var process = processForPid(systemEvents, pid);
  if (!process) throw new Error("The requested application process is no longer running");
  return JSON.stringify({
    application: text(process.name(), 240),
    pid: Number(process.unixId()),
    frontmost: Boolean(process.frontmost())
  });
}
"#;

const ACTIVATE_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function processForPid(systemEvents, pid) {
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Number(candidate.unixId()) === pid) return candidate;
    } catch (_) {}
  }
  return null;
}
function run(argv) {
  var pid = Number.parseInt(argv[0] || "0", 10);
  var expectedName = String(argv[1] || "");
  var previousPid = Number.parseInt(argv[2] || "0", 10);
  var previousName = String(argv[3] || "");
  if (!Number.isFinite(pid) || pid < 1) throw new Error("A positive application PID is required");
  if (!expectedName) throw new Error("An approved application identity is required");
  if (!Number.isFinite(previousPid) || previousPid < 1 || !previousName) {
    throw new Error("A valid previous foreground application identity is required");
  }
  var systemEvents = Application("System Events");
  var process = processForPid(systemEvents, pid);
  if (!process) throw new Error("The requested application process is no longer running");
  var actualName = text(process.name(), 240);
  if (actualName !== expectedName) throw new Error("The approved application identity changed before activation");
  var previous = processForPid(systemEvents, previousPid);
  if (!previous || text(previous.name(), 240) !== previousName || !Boolean(previous.frontmost())) {
    throw new Error("The frontmost application changed before activation");
  }
  process.frontmost.set(true);
  return JSON.stringify({
    application: actualName,
    pid: Number(process.unixId()),
    activated: true
  });
}
"#;

const FRONTMOST_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function run() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Boolean(candidate.frontmost())) {
        var pid = Number(candidate.unixId());
        var application = text(candidate.name(), 240);
        if (!Number.isFinite(pid) || pid < 1 || !application) {
          throw new Error("The frontmost application identity is invalid");
        }
        return JSON.stringify({application: application, pid: pid});
      }
    } catch (error) {
      if (String(error).indexOf("identity is invalid") >= 0) throw error;
    }
  }
  throw new Error("No frontmost application process is available");
}
"#;

const RESTORE_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function processForPid(systemEvents, pid) {
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Number(candidate.unixId()) === pid) return candidate;
    } catch (_) {}
  }
  return null;
}
function run(argv) {
  var previousPid = Number.parseInt(argv[0] || "0", 10);
  var previousName = String(argv[1] || "");
  var targetPid = Number.parseInt(argv[2] || "0", 10);
  var targetName = String(argv[3] || "");
  if (!Number.isFinite(previousPid) || previousPid < 1 || !previousName ||
      !Number.isFinite(targetPid) || targetPid < 1 || !targetName) {
    throw new Error("Application activation rollback identities are invalid");
  }
  if (previousPid === targetPid && previousName === targetName) {
    return JSON.stringify({attempted: false, restored: true, reason: "activation_did_not_change_frontmost_application"});
  }
  var systemEvents = Application("System Events");
  var target = processForPid(systemEvents, targetPid);
  if (!target || text(target.name(), 240) !== targetName || !Boolean(target.frontmost())) {
    return JSON.stringify({attempted: false, restored: false, reason: "foreground_changed_after_activation"});
  }
  var previous = processForPid(systemEvents, previousPid);
  if (!previous || text(previous.name(), 240) !== previousName) {
    return JSON.stringify({attempted: false, restored: false, reason: "previous_application_identity_unavailable"});
  }
  previous.frontmost.set(true);
  if (!Boolean(previous.frontmost())) {
    return JSON.stringify({attempted: true, restored: false, reason: "platform_refused_restore"});
  }
  return JSON.stringify({attempted: true, restored: true, reason: "cancelled_activation_restored"});
}
"#;

pub(super) fn tool_definitions(include_control: bool) -> Vec<Value> {
    tool_definitions_for_platform(include_control, current_platform_name())
}

fn tool_definitions_for_platform(include_control: bool, platform: &str) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "computer_list_windows",
            "description": "Read-only desktop observation on the current supported platform: list visible application windows, titles, positions, and sizes. Does not click, type, or read text-field contents.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_WINDOW_LIMIT, "default": DEFAULT_WINDOW_LIMIT}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_window_layout",
            "description": "Read-only capture of a short-lived opaque layout snapshot for at most 8 ordinary visible top-level windows on the current desktop. Only non-minimized, non-fullscreen/non-maximized windows with writable native position and size are included. The model receives only a snapshot ID, SHA-256, counts, and application summary; native window identities and coordinates remain in volatile Local Connector memory for 10 minutes and are never persisted.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_inspect_frontmost_window",
            "description": "Read-only bounded Accessibility/UI Automation inspection of the frontmost window on the current supported platform. Editable and secure text values are redacted; only reviewed visible control metadata is returned.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_depth": {"type": "integer", "minimum": 1, "maximum": MAX_TREE_DEPTH, "default": DEFAULT_TREE_DEPTH},
                    "max_nodes": {"type": "integer", "minimum": 1, "maximum": MAX_TREE_NODES, "default": DEFAULT_TREE_NODES}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_main_display",
            "description": "Read-only screenshot observation of the main display on the current supported platform. The image is delivered only as transient model input, is never persisted in tool history, and may contain sensitive visible information. Does not click, type, or change desktop state.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_frontmost_window",
            "description": "Read-only screenshot observation limited to the current frontmost visible window on the current supported platform. The exact window identity and geometry are revalidated after capture; any foreground or layout drift fails closed. The image is transient model input and is never persisted in tool history.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_list_displays",
            "description": "Read-only display discovery on the current supported platform. Returns stable-for-this-moment indexes and display identities, global coordinate bounds, pixel dimensions, scale, rotation when available, and main-display status. Re-list after display hot-plug or arrangement changes.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_display",
            "description": "Read-only screenshot observation of one currently active display selected by the 1-based display_index returned by computer_list_displays. The image is transient model input and is never persisted in tool history.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "display_index": {"type": "integer", "minimum": 1, "maximum": 16}
                },
                "required": ["display_index"],
                "additionalProperties": false
            }
        }),
    ];
    if include_control {
        tools.extend([
            json!({
                "name": "computer_click",
                "description": "Perform one left/right click or one left-button double-click at a display-local point on the current supported platform. Omit display_index for the main display, or use the current 1-based index from computer_list_displays. Every exact button, click count, point, and display requires explicit local user approval; display geometry is revalidated after approval. A best-effort transient post-action screenshot is attached without persisting pixels; if observation fails, the result still records that the click already ran and must not be replayed automatically.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "display_index": {"type": "integer", "minimum": 1, "maximum": 16, "description": "Optional 1-based active display index; defaults to the main display."},
                        "x": {"type": "number", "description": "Display-local x coordinate in platform desktop units."},
                        "y": {"type": "number", "description": "Display-local y coordinate in platform desktop units."},
                        "button": {"type": "string", "enum": ["left", "right"], "default": "left"},
                        "click_count": {"type": "integer", "enum": [1, 2], "default": 1, "description": "Two clicks are supported only with the left button."}
                    },
                    "required": ["x", "y"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_drag",
                "description": "Perform one bounded left-button drag between two display-local points on the same active display. Duration is limited to 80-1000 ms. The exact path requires explicit local user approval, display identity and geometry are revalidated after approval, and cancellation forces mouse-up before returning. A best-effort transient post-action screenshot is attached without persisting pixels; observation failure never causes an automatic drag replay.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "display_index": {"type": "integer", "minimum": 1, "maximum": 16, "description": "Optional 1-based active display index; defaults to the main display."},
                        "start_x": {"type": "number", "description": "Display-local starting x coordinate in platform desktop units."},
                        "start_y": {"type": "number", "description": "Display-local starting y coordinate in platform desktop units."},
                        "end_x": {"type": "number", "description": "Display-local ending x coordinate in platform desktop units."},
                        "end_y": {"type": "number", "description": "Display-local ending y coordinate in platform desktop units."},
                        "duration_ms": {"type": "integer", "minimum": 80, "maximum": 1000, "default": 300}
                    },
                    "required": ["start_x", "start_y", "end_x", "end_y"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_press_key",
                "description": "Press one reviewed navigation key on the current supported platform, optionally with reviewed modifiers. Use computer_type_text for approved bounded text entry into a verified non-secure editable control. Arbitrary letter key codes are not supported. Enter, Backspace, and every modified shortcut additionally require the user to type a one-time random confirmation challenge; Computer Use actions cannot be approved for the whole session. A best-effort transient post-action screenshot is attached and the action is never replayed merely because observation failed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {"type": "string", "enum": ["enter", "tab", "space", "escape", "backspace", "left", "right", "up", "down", "home", "end", "page_up", "page_down"]},
                        "modifiers": {
                            "type": "array",
                            "items": {"type": "string", "enum": ["command", "control", "option", "shift"]},
                            "maxItems": 4,
                            "uniqueItems": true,
                            "default": []
                        }
                    },
                    "required": ["key"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_type_text",
                "description": "Type bounded Unicode text into the currently focused non-secure editable text control on the current supported platform. macOS requires a live frontmost Accessibility identity, focus, enabled and visible bounds, then either a writable native text role or an explicit AXIsEditable rich-text target with writable AXSelectedTextRange; the same focused and editable AX elements are compared again immediately before input. Windows requires matching foreground PID, focus, enabled and visible bounds, explicit non-password state, then either Edit plus writable ValuePattern or Document/Pane/Custom plus live TextEditPattern; the same UI Automation element is compared again before SendInput. Any unknown state fails closed. The exact text is shown only in the local approval request, while persistent approval history and structured tool results retain only length and SHA-256. Approval additionally requires the user to type a one-time random confirmation challenge and can never be remembered for the session. A transient post-action screenshot may visually contain the updated control but is never persisted.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": {"type": "string", "minLength": 1, "maxLength": 2048}
                    },
                    "required": ["text"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_scroll",
                "description": "Post one bounded scroll event at the current pointer target on the current supported platform. Positive delta_y scrolls up and positive delta_x scrolls right. Every exact scroll requires explicit local user approval. A best-effort transient post-action screenshot is attached and observation failure never triggers an automatic repeat.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "delta_y": {"type": "integer", "minimum": -1200, "maximum": 1200, "default": 0},
                        "delta_x": {"type": "integer", "minimum": -1200, "maximum": 1200, "default": 0}
                    },
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_activate_application",
                "description": "Bring one already-running application process to the front by its PID from computer_list_windows. The Local Connector resolves the real process identity before showing the mandatory approval and rechecks it during execution; model-provided application names are not accepted. If the action is cancelled while still in flight after activation, ChatOS attempts to restore the exact previous foreground application only when the approved target remains foreground and both identities still match; a user or system foreground change disables rollback. This recovery does not undo application content or arbitrary window changes. A best-effort transient post-action screenshot is attached and activation is not replayed if observation fails.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pid": {"type": "integer", "minimum": 1, "maximum": 2147483647}
                    },
                    "required": ["pid"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_set_frontmost_window_bounds",
                "description": "Move and resize only the current frontmost non-fullscreen, non-maximized window to one reviewed global desktop rectangle. Approval binds the exact process, native window identity, original state and geometry, and requested rectangle. The target must leave at least 64 x 64 desktop units visible on one active display. Identity, foreground, state, capability, display-layout, or geometry drift fails closed; partial platform failures attempt an identity-bound restoration and are never automatically replayed. After settling, ChatOS revalidates the requested state and captures the exact frontmost window rather than assuming it remained on the main display.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "x": {"type": "integer", "minimum": MIN_WINDOW_COORDINATE, "maximum": MAX_WINDOW_COORDINATE, "description": "Global desktop x coordinate for the window's top-left corner."},
                        "y": {"type": "integer", "minimum": MIN_WINDOW_COORDINATE, "maximum": MAX_WINDOW_COORDINATE, "description": "Global desktop y coordinate for the window's top-left corner."},
                        "width": {"type": "integer", "minimum": MIN_WINDOW_DIMENSION, "maximum": MAX_WINDOW_DIMENSION},
                        "height": {"type": "integer", "minimum": MIN_WINDOW_DIMENSION, "maximum": MAX_WINDOW_DIMENSION}
                    },
                    "required": ["x", "y", "width", "height"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_set_frontmost_window_fullscreen",
                "description": "macOS only: set the exact current frontmost Accessibility window's native AXFullScreen state. Approval binds its process, AX window number, original geometry/state, and requested state. The AXFullScreen attribute must be explicitly writable, and foreground or identity drift fails closed. This does not simulate the green button or send a keyboard shortcut. Post-action observation revalidates the exact window and requested fullscreen state before and after capturing that window.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "fullscreen": {"type": "boolean"}
                    },
                    "required": ["fullscreen"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_set_frontmost_window_maximized",
                "description": "Windows only: maximize or restore the exact current foreground HWND. This is standard Windows maximize/restore, not true application fullscreen. Approval binds HWND, PID/process image, original geometry/state, and requested state; foreground, identity, state, or geometry drift fails closed and cancellation attempts to restore the approved prior state. Post-action observation captures only that exact foreground window after revalidating its requested maximize state.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "maximized": {"type": "boolean"}
                    },
                    "required": ["maximized"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_restore_window_layout",
                "description": "Restore one exact short-lived layout snapshot created by computer_capture_window_layout. The request accepts only the opaque snapshot ID and its SHA-256, never PID, HWND/AX window ID, application identity, or coordinates. A fresh local approval plus one-time typed confirmation is mandatory. Display-layout drift or any missing/changed/non-ordinary window fails the whole batch before mutation; partial execution rolls back only windows changed by this batch whose exact identity and target geometry still match. Application content, navigation, text, and document state are never rolled back, and automatic replay is always unsafe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "snapshot_id": {"type": "string", "minLength": 36, "maxLength": 36},
                        "snapshot_sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
                    },
                    "required": ["snapshot_id", "snapshot_sha256"],
                    "additionalProperties": false
                }
            }),
        ]);
    }
    filter_tools_for_platform(&mut tools, platform);
    tools
}

fn filter_tools_for_platform(tools: &mut Vec<Value>, platform: &str) {
    tools.retain(|tool| {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        match name {
            "computer_set_frontmost_window_fullscreen" => platform == "macos",
            "computer_set_frontmost_window_maximized" => platform == "windows",
            _ => true,
        }
    });
}

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

fn format_audit_number(value: f64) -> String {
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

#[cfg(target_os = "macos")]
pub(super) fn dependency_error() -> Option<String> {
    helper::dependency_error()
}

#[cfg(not(target_os = "macos"))]
pub(super) fn dependency_error() -> Option<String> {
    dependency_error_local()
}

fn dependency_error_local() -> Option<String> {
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

fn screen_capture_dependency_error_local() -> Option<String> {
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

#[cfg(not(target_os = "macos"))]
fn macos_screen_capture_is_trusted() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
fn macos_accessibility_is_trusted() -> bool {
    false
}

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
fn preflight_window_layout_snapshot(snapshot: &WindowLayoutSnapshot) -> Result<()> {
    helper::preflight_window_layout(snapshot)
}

#[cfg(target_os = "windows")]
fn preflight_window_layout_snapshot(snapshot: &WindowLayoutSnapshot) -> Result<()> {
    preflight_window_layout_snapshot_local(snapshot).map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn preflight_window_layout_snapshot(_snapshot: &WindowLayoutSnapshot) -> Result<()> {
    Err(anyhow!(
        "Computer Use window layout restore is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn preflight_window_layout_snapshot_local(snapshot: &WindowLayoutSnapshot) -> Result<Value> {
    snapshot.validate()?;
    execute_jxa_action(
        PREFLIGHT_WINDOW_LAYOUT_JXA,
        &[serde_json::to_string(snapshot)?],
    )
}

#[cfg(target_os = "windows")]
fn preflight_window_layout_snapshot_local(snapshot: &WindowLayoutSnapshot) -> Result<Value> {
    windows::preflight_window_layout(snapshot)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn preflight_window_layout_snapshot_local(_snapshot: &WindowLayoutSnapshot) -> Result<Value> {
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

fn execute_local(operation: &str, arguments: &Value) -> Result<Value> {
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

fn execute_approved_local(
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

#[derive(Debug, Clone)]
enum PostActionObservationTarget {
    MainDisplay,
    ApprovedDisplay(ApprovedDisplayGuard),
    FrontmostWindow(FrontmostWindowObservationGuard),
}

#[derive(Debug, Clone)]
struct FrontmostWindowObservationGuard {
    platform: String,
    application: String,
    pid: u32,
    window_id: String,
}

#[derive(Debug, Clone)]
enum WindowControlRollbackGuard {
    Bounds {
        request: WindowBoundsRequest,
        approved: ApprovedFrontmostWindowGuard,
    },
    Fullscreen {
        fullscreen: bool,
        approved: ApprovedFrontmostWindowGuard,
    },
    Maximized {
        maximized: bool,
        approved: ApprovedFrontmostWindowGuard,
    },
}

impl PostActionObservationTarget {
    fn requested_index(&self) -> Option<u32> {
        match self {
            Self::MainDisplay => None,
            Self::ApprovedDisplay(display) => Some(display.index),
            Self::FrontmostWindow(_) => None,
        }
    }

    fn metadata(&self) -> Value {
        match self {
            Self::MainDisplay => json!({"scope": "main_display"}),
            Self::ApprovedDisplay(display) => json!({
                "scope": "approved_display",
                "display_index": display.index,
                "display_id": display.display_id,
            }),
            Self::FrontmostWindow(window) => json!({
                "scope": "frontmost_window",
                "platform": window.platform,
                "application": window.application,
                "pid": window.pid,
                "window_id": window.window_id,
            }),
        }
    }

    fn matches_capture(&self, capture: &Value) -> bool {
        match self {
            Self::MainDisplay => capture
                .get("is_main")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            Self::ApprovedDisplay(display) => {
                capture.get("display_index").and_then(Value::as_u64)
                    == Some(u64::from(display.index))
                    && capture.get("display_id").and_then(Value::as_u64)
                        == Some(u64::from(display.display_id))
            }
            Self::FrontmostWindow(window) => {
                capture.get("capture_scope").and_then(Value::as_str) == Some("frontmost_window")
                    && capture.get("platform").and_then(Value::as_str)
                        == Some(window.platform.as_str())
                    && capture.get("application").and_then(Value::as_str)
                        == Some(window.application.as_str())
                    && capture.get("pid").and_then(Value::as_u64) == Some(u64::from(window.pid))
                    && capture.get("window_id").and_then(Value::as_str)
                        == Some(window.window_id.as_str())
            }
        }
    }

    fn mismatch_reason(&self) -> &'static str {
        match self {
            Self::MainDisplay | Self::ApprovedDisplay(_) => "display_identity_changed",
            Self::FrontmostWindow(_) => "frontmost_window_identity_changed",
        }
    }
}

impl WindowControlRollbackGuard {
    fn observation_target(&self) -> PostActionObservationTarget {
        let approved = match self {
            Self::Bounds { approved, .. }
            | Self::Fullscreen { approved, .. }
            | Self::Maximized { approved, .. } => approved,
        };
        PostActionObservationTarget::FrontmostWindow(FrontmostWindowObservationGuard {
            platform: approved.platform.clone(),
            application: approved.application.clone(),
            pid: approved.pid,
            window_id: approved.window_id.clone(),
        })
    }

    fn matches_target_identity(&self, current: &ApprovedFrontmostWindowGuard) -> bool {
        let approved = match self {
            Self::Bounds { approved, .. }
            | Self::Fullscreen { approved, .. }
            | Self::Maximized { approved, .. } => approved,
        };
        current.platform == approved.platform
            && current.application == approved.application
            && current.pid == approved.pid
            && current.window_id == approved.window_id
    }

    fn matches_applied_state(&self, current: &ApprovedFrontmostWindowGuard) -> bool {
        if !self.matches_target_identity(current) {
            return false;
        }
        match self {
            Self::Bounds { request, .. } => {
                current.position == [f64::from(request.x), f64::from(request.y)]
                    && current.size == [f64::from(request.width), f64::from(request.height)]
                    && current.fullscreen != Some(true)
                    && current.maximized != Some(true)
            }
            Self::Fullscreen { fullscreen, .. } => current.fullscreen == Some(*fullscreen),
            Self::Maximized { maximized, .. } => current.maximized == Some(*maximized),
        }
    }
}

fn attach_post_action_observation(
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

fn attach_activation_post_action_observation(
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

fn attach_window_post_action_observation(
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

fn with_application_activation_recovery(mut result: Value, rollback: Value) -> Value {
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

fn build_post_action_result(
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

#[derive(Debug)]
struct ClickAction<'a> {
    display: DisplayTarget,
    x: f64,
    y: f64,
    global_x: f64,
    global_y: f64,
    button: &'a str,
    click_count: u32,
}

#[derive(Debug)]
struct DragAction {
    display: DisplayTarget,
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    global_start_x: f64,
    global_start_y: f64,
    global_end_x: f64,
    global_end_y: f64,
    duration_ms: u64,
}

#[derive(Debug)]
struct KeyAction<'a> {
    key: &'a str,
    modifiers: Vec<&'a str>,
}

#[derive(Debug)]
struct TypedTextAction<'a> {
    text: &'a str,
    utf16: Vec<u16>,
    character_count: usize,
    sha256: String,
}

#[derive(Debug)]
struct ScrollAction {
    delta_y: i32,
    delta_x: i32,
}

#[derive(Debug, Clone, Copy)]
struct WindowBoundsRequest {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowLayoutReference {
    snapshot_id: String,
    snapshot_sha256: String,
}

impl WindowBoundsRequest {
    fn geometry(&self) -> String {
        format!("{} x {} @ {}, {}", self.width, self.height, self.x, self.y)
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct ApplicationIdentity {
    pid: u32,
    application: String,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct ApplicationActivationRollbackGuard {
    previous: ApplicationIdentity,
    target: ApplicationIdentity,
    changed_frontmost_application: bool,
}

#[cfg(target_os = "windows")]
type ApplicationActivationRollbackGuard = windows::ApplicationActivationRollbackGuard;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[derive(Debug, Clone)]
struct ApplicationActivationRollbackGuard;

fn parse_click(arguments: &Value) -> Result<ClickAction<'_>> {
    reject_unknown_fields(
        arguments,
        &["display_index", "x", "y", "button", "click_count"],
    )?;
    let display = resolve_display(arguments.get("display_index"))?;
    let x = finite_number(arguments, "x")?;
    let y = finite_number(arguments, "y")?;
    let button = arguments
        .get("button")
        .and_then(Value::as_str)
        .unwrap_or("left");
    if !matches!(button, "left" | "right") {
        return Err(anyhow!("button must be left or right"));
    }
    let click_count = parse_click_count(arguments, button)?;
    if x < 0.0 || x >= display.width || y < 0.0 || y >= display.height {
        return Err(anyhow!(
            "click coordinates must be inside the selected display bounds"
        ));
    }
    Ok(ClickAction {
        display: display.clone(),
        x,
        y,
        global_x: display.origin_x + x,
        global_y: display.origin_y + y,
        button,
        click_count,
    })
}

fn parse_click_count(arguments: &Value, button: &str) -> Result<u32> {
    let click_count = arguments
        .get("click_count")
        .map(|value| {
            value
                .as_u64()
                .and_then(|count| u32::try_from(count).ok())
                .filter(|count| matches!(count, 1 | 2))
                .ok_or_else(|| anyhow!("click_count must be 1 or 2"))
        })
        .transpose()?
        .unwrap_or(1);
    if button == "right" && click_count != 1 {
        return Err(anyhow!("right-button clicks require click_count=1"));
    }
    Ok(click_count)
}

fn click_approval_arguments(action: &ClickAction<'_>) -> Result<Vec<String>> {
    Ok(vec![
        format!("--display-index={}", action.display.index),
        format!("--x={}", action.x),
        format!("--y={}", action.y),
        format!("--button={}", action.button),
        format!("--click-count={}", action.click_count),
        display_approval_argument(&action.display)?,
    ])
}

fn parse_drag(arguments: &Value) -> Result<DragAction> {
    reject_unknown_fields(
        arguments,
        &[
            "display_index",
            "start_x",
            "start_y",
            "end_x",
            "end_y",
            "duration_ms",
        ],
    )?;
    let display = resolve_display(arguments.get("display_index"))?;
    let start_x = finite_number(arguments, "start_x")?;
    let start_y = finite_number(arguments, "start_y")?;
    let end_x = finite_number(arguments, "end_x")?;
    let end_y = finite_number(arguments, "end_y")?;
    for (label, x, y) in [("drag start", start_x, start_y), ("drag end", end_x, end_y)] {
        if x < 0.0 || x >= display.width || y < 0.0 || y >= display.height {
            return Err(anyhow!(
                "{label} coordinates must be inside the selected display bounds"
            ));
        }
    }
    if start_x == end_x && start_y == end_y {
        return Err(anyhow!("drag start and end coordinates must differ"));
    }
    let duration_ms = bounded_integer(
        arguments,
        "duration_ms",
        DEFAULT_DRAG_DURATION_MS,
        MIN_DRAG_DURATION_MS,
        MAX_DRAG_DURATION_MS,
    )?;
    Ok(DragAction {
        display: display.clone(),
        start_x,
        start_y,
        end_x,
        end_y,
        global_start_x: display.origin_x + start_x,
        global_start_y: display.origin_y + start_y,
        global_end_x: display.origin_x + end_x,
        global_end_y: display.origin_y + end_y,
        duration_ms,
    })
}

fn display_approval_argument(display: &DisplayTarget) -> Result<String> {
    Ok(format!(
        "--display-json={}",
        serde_json::to_string(&ApprovedDisplayGuard::from(display))?
    ))
}

fn approved_display_guard(
    approved_command_args: Option<&[String]>,
) -> Result<ApprovedDisplayGuard> {
    let arguments =
        approved_command_args.ok_or_else(|| anyhow!("approved display identity is missing"))?;
    let encoded = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--display-json="))
        .ok_or_else(|| anyhow!("approved display identity is missing"))?;
    serde_json::from_str(encoded).context("decode approved display identity")
}

fn validate_approved_display(
    display: &DisplayTarget,
    approved_command_args: Option<&[String]>,
) -> Result<()> {
    let approved = approved_display_guard(approved_command_args)?;
    let current = ApprovedDisplayGuard::from(display);
    if approved != current {
        return Err(anyhow!(
            "selected display identity or geometry changed after approval; observe and approve again"
        ));
    }
    Ok(())
}

fn ensure_action_not_cancelled(action_cancelled: Option<&AtomicBool>) -> Result<()> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return Err(anyhow!("Computer Use action was cancelled"));
    }
    Ok(())
}

fn drag_step_count(duration_ms: u64) -> u32 {
    ((duration_ms.saturating_add(15) / 16) as u32).clamp(4, MAX_DRAG_STEPS)
}

fn parse_key_action(arguments: &Value) -> Result<KeyAction<'_>> {
    reject_unknown_fields(arguments, &["key", "modifiers"])?;
    let key = arguments
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("key is required"))?;
    key_code(key)?;
    let mut modifiers = Vec::new();
    if let Some(values) = arguments.get("modifiers") {
        let values = values
            .as_array()
            .ok_or_else(|| anyhow!("modifiers must be an array"))?;
        if values.len() > 4 {
            return Err(anyhow!("modifiers may contain at most 4 values"));
        }
        for value in values {
            let modifier = value
                .as_str()
                .ok_or_else(|| anyhow!("modifier values must be strings"))?;
            if !matches!(modifier, "command" | "control" | "option" | "shift") {
                return Err(anyhow!("unsupported modifier: {modifier}"));
            }
            if modifiers.contains(&modifier) {
                return Err(anyhow!("duplicate modifier: {modifier}"));
            }
            modifiers.push(modifier);
        }
    }
    modifiers.sort_unstable();
    Ok(KeyAction { key, modifiers })
}

fn key_confirmation_risk(action: &KeyAction<'_>) -> Option<&'static str> {
    if action.key == "enter" {
        Some("submit_or_activate")
    } else if action.key == "backspace" {
        Some("destructive_key")
    } else if !action.modifiers.is_empty() {
        Some("application_shortcut")
    } else {
        None
    }
}

fn parse_typed_text(arguments: &Value) -> Result<TypedTextAction<'_>> {
    reject_unknown_fields(arguments, &["text"])?;
    let text = arguments
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("text is required"))?;
    if text.is_empty() {
        return Err(anyhow!("text must not be empty"));
    }
    let character_count = text.chars().count();
    if character_count > MAX_TYPED_TEXT_CHARS {
        return Err(anyhow!(
            "text exceeds the {MAX_TYPED_TEXT_CHARS} character limit"
        ));
    }
    if text.chars().any(is_unsafe_typed_character) {
        return Err(anyhow!(
            "text contains a control or invisible formatting character"
        ));
    }
    let utf16 = text.encode_utf16().collect::<Vec<_>>();
    if utf16.len() > MAX_TYPED_TEXT_UTF16_UNITS {
        return Err(anyhow!(
            "text exceeds the {MAX_TYPED_TEXT_UTF16_UNITS} UTF-16 unit limit"
        ));
    }
    Ok(TypedTextAction {
        text,
        utf16,
        character_count,
        sha256: hex::encode(Sha256::digest(text.as_bytes())),
    })
}

fn is_unsafe_typed_character(character: char) -> bool {
    character.is_control()
        || matches!(
            character as u32,
            0x200B..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069 | 0xFEFF
        )
}

fn parse_scroll(arguments: &Value) -> Result<ScrollAction> {
    reject_unknown_fields(arguments, &["delta_y", "delta_x"])?;
    let delta_y =
        bounded_signed_integer(arguments, "delta_y", 0, -MAX_SCROLL_DELTA, MAX_SCROLL_DELTA)?;
    let delta_x =
        bounded_signed_integer(arguments, "delta_x", 0, -MAX_SCROLL_DELTA, MAX_SCROLL_DELTA)?;
    if delta_y == 0 && delta_x == 0 {
        return Err(anyhow!("at least one scroll delta must be non-zero"));
    }
    Ok(ScrollAction {
        delta_y: delta_y as i32,
        delta_x: delta_x as i32,
    })
}

fn parse_application_pid(arguments: &Value) -> Result<u32> {
    reject_unknown_fields(arguments, &["pid"])?;
    let pid = arguments
        .get("pid")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("pid must be a positive integer"))?;
    if pid == 0 || pid > i32::MAX as u64 {
        return Err(anyhow!("pid must be between 1 and {}", i32::MAX));
    }
    Ok(pid as u32)
}

fn parse_window_bounds_request(arguments: &Value) -> Result<WindowBoundsRequest> {
    reject_unknown_fields(arguments, &["x", "y", "width", "height"])?;
    Ok(WindowBoundsRequest {
        x: required_bounded_i32(arguments, "x", MIN_WINDOW_COORDINATE, MAX_WINDOW_COORDINATE)?,
        y: required_bounded_i32(arguments, "y", MIN_WINDOW_COORDINATE, MAX_WINDOW_COORDINATE)?,
        width: required_bounded_i32(
            arguments,
            "width",
            MIN_WINDOW_DIMENSION,
            MAX_WINDOW_DIMENSION,
        )?,
        height: required_bounded_i32(
            arguments,
            "height",
            MIN_WINDOW_DIMENSION,
            MAX_WINDOW_DIMENSION,
        )?,
    })
}

fn parse_window_fullscreen_request(arguments: &Value) -> Result<bool> {
    reject_unknown_fields(arguments, &["fullscreen"])?;
    arguments
        .get("fullscreen")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("fullscreen must be a boolean"))
}

fn parse_window_maximized_request(arguments: &Value) -> Result<bool> {
    reject_unknown_fields(arguments, &["maximized"])?;
    arguments
        .get("maximized")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("maximized must be a boolean"))
}

fn parse_window_layout_reference(arguments: &Value) -> Result<WindowLayoutReference> {
    reject_unknown_fields(arguments, &["snapshot_id", "snapshot_sha256"])?;
    let snapshot_id = arguments
        .get("snapshot_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("snapshot_id is required"))?;
    let parsed_id = uuid::Uuid::parse_str(snapshot_id)
        .map_err(|_| anyhow!("snapshot_id must be a canonical UUID"))?;
    if parsed_id.hyphenated().to_string() != snapshot_id {
        return Err(anyhow!("snapshot_id must be a canonical lowercase UUID"));
    }
    let snapshot_sha256 = arguments
        .get("snapshot_sha256")
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .ok_or_else(|| anyhow!("snapshot_sha256 must be 64 lowercase hexadecimal characters"))?;
    Ok(WindowLayoutReference {
        snapshot_id: snapshot_id.to_string(),
        snapshot_sha256: snapshot_sha256.to_string(),
    })
}

fn window_layout_sha256(snapshot: &WindowLayoutSnapshot) -> Result<String> {
    let payload = serde_json::to_vec(&(
        snapshot.schema_version,
        snapshot.snapshot_id.as_str(),
        snapshot.platform.as_str(),
        &snapshot.display_layout,
        &snapshot.windows,
        snapshot.excluded_window_count,
        snapshot.truncated,
    ))
    .context("encode window layout snapshot hash payload")?;
    Ok(hex::encode(Sha256::digest(payload)))
}

fn validate_window_layout_geometry(
    window: &ApprovedWindowLayoutGuard,
    display_layout: &[ApprovedDisplayGuard],
) -> Result<()> {
    let left = window.position[0];
    let top = window.position[1];
    let right = left + window.size[0];
    let bottom = top + window.size[1];
    if !display_layout.iter().any(|display| {
        let overlap_width =
            right.min(display.origin_x + display.width) - left.max(display.origin_x);
        let overlap_height =
            bottom.min(display.origin_y + display.height) - top.max(display.origin_y);
        overlap_width >= MIN_WINDOW_DIMENSION as f64
            && overlap_height >= MIN_WINDOW_DIMENSION as f64
    }) {
        return Err(anyhow!(
            "snapshotted window geometry must leave at least {MIN_WINDOW_DIMENSION} x {MIN_WINDOW_DIMENSION} desktop units visible"
        ));
    }
    Ok(())
}

fn window_layout_snapshot_store() -> &'static Mutex<BTreeMap<String, StoredWindowLayoutSnapshot>> {
    static STORE: OnceLock<Mutex<BTreeMap<String, StoredWindowLayoutSnapshot>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn prune_expired_window_layout_snapshots(
    snapshots: &mut BTreeMap<String, StoredWindowLayoutSnapshot>,
    now: Instant,
) {
    snapshots.retain(|_, stored| {
        now.checked_duration_since(stored.captured_at)
            .is_some_and(|age| age <= WINDOW_LAYOUT_SNAPSHOT_TTL)
    });
}

fn evict_window_layout_snapshot_for_insert(
    snapshots: &mut BTreeMap<String, StoredWindowLayoutSnapshot>,
) {
    while snapshots.len() >= MAX_WINDOW_LAYOUT_SNAPSHOTS {
        let Some(oldest) = snapshots
            .iter()
            .min_by_key(|(_, stored)| stored.captured_at)
            .map(|(snapshot_id, _)| snapshot_id.clone())
        else {
            break;
        };
        snapshots.remove(oldest.as_str());
    }
}

fn store_window_layout_snapshot(snapshot: WindowLayoutSnapshot) -> Result<()> {
    snapshot.validate()?;
    let now = Instant::now();
    let mut snapshots = window_layout_snapshot_store()
        .lock()
        .map_err(|_| anyhow!("window layout snapshot store is unavailable"))?;
    prune_expired_window_layout_snapshots(&mut snapshots, now);
    evict_window_layout_snapshot_for_insert(&mut snapshots);
    snapshots.insert(
        snapshot.snapshot_id.clone(),
        StoredWindowLayoutSnapshot {
            captured_at: now,
            snapshot,
        },
    );
    Ok(())
}

fn stored_window_layout_snapshot(
    reference: &WindowLayoutReference,
) -> Result<WindowLayoutSnapshot> {
    let now = Instant::now();
    let mut snapshots = window_layout_snapshot_store()
        .lock()
        .map_err(|_| anyhow!("window layout snapshot store is unavailable"))?;
    prune_expired_window_layout_snapshots(&mut snapshots, now);
    let snapshot = snapshots
        .get(reference.snapshot_id.as_str())
        .map(|stored| stored.snapshot.clone())
        .ok_or_else(|| anyhow!("window layout snapshot is missing or expired; capture it again"))?;
    if snapshot.snapshot_sha256 != reference.snapshot_sha256 {
        return Err(anyhow!("window layout snapshot SHA-256 does not match"));
    }
    snapshot.validate()?;
    Ok(snapshot)
}

fn window_layout_approval_argument(snapshot: &WindowLayoutSnapshot) -> Result<String> {
    snapshot.validate()?;
    Ok(format!(
        "--window-layout-json={}",
        serde_json::to_string(snapshot)?
    ))
}

fn approved_window_layout_snapshot(
    approved_command_args: Option<&[String]>,
) -> Result<WindowLayoutSnapshot> {
    let arguments = approved_command_args
        .ok_or_else(|| anyhow!("approved window layout snapshot is missing"))?;
    let encoded = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--window-layout-json="))
        .ok_or_else(|| anyhow!("approved window layout snapshot is missing"))?;
    let snapshot = serde_json::from_str::<WindowLayoutSnapshot>(encoded)
        .context("decode approved window layout snapshot")?;
    snapshot.validate()?;
    Ok(snapshot)
}

fn consume_approved_window_layout_snapshot(
    arguments: &Value,
    approved_command_args: Option<&[String]>,
) -> Result<()> {
    let reference = parse_window_layout_reference(arguments)?;
    let approved = approved_window_layout_snapshot(approved_command_args)?;
    if approved.snapshot_id != reference.snapshot_id
        || approved.snapshot_sha256 != reference.snapshot_sha256
    {
        return Err(anyhow!(
            "approved window layout snapshot does not match the requested opaque reference"
        ));
    }
    let now = Instant::now();
    let mut snapshots = window_layout_snapshot_store()
        .lock()
        .map_err(|_| anyhow!("window layout snapshot store is unavailable"))?;
    prune_expired_window_layout_snapshots(&mut snapshots, now);
    let stored = snapshots
        .get(reference.snapshot_id.as_str())
        .ok_or_else(|| anyhow!("window layout snapshot is missing or expired; capture it again"))?;
    if stored.snapshot != approved {
        return Err(anyhow!(
            "approved window layout snapshot no longer matches volatile Local Connector state"
        ));
    }
    snapshots.remove(reference.snapshot_id.as_str());
    Ok(())
}

fn window_layout_application_summary(windows: &[ApprovedWindowLayoutGuard]) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    for window in windows {
        *counts.entry(window.application.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(application, count)| format!("{} ({count})", safe_approval_label(application)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn finalize_window_layout_capture(result: Value) -> Result<Value> {
    let payload = serde_json::from_value::<WindowLayoutCapturePayload>(result)
        .context("decode native window layout capture")?;
    if payload.platform != current_platform_name()
        || payload.windows.is_empty()
        || payload.windows.len() > MAX_WINDOW_LAYOUT_WINDOWS
        || payload
            .windows
            .iter()
            .any(|window| window.platform != payload.platform || window.validate().is_err())
    {
        return Err(anyhow!("native window layout capture is invalid"));
    }
    let mut identities = BTreeMap::<(&str, u32, &str), ()>::new();
    for window in &payload.windows {
        if identities
            .insert(
                (
                    window.process_identity.as_str(),
                    window.pid,
                    window.window_id.as_str(),
                ),
                (),
            )
            .is_some()
        {
            return Err(anyhow!(
                "native window layout capture contains duplicate identities"
            ));
        }
    }
    let display_layout = payload.display_layout;
    for window in &payload.windows {
        validate_window_layout_geometry(window, &display_layout)?;
    }
    let mut snapshot = WindowLayoutSnapshot {
        schema_version: WINDOW_LAYOUT_SCHEMA_VERSION,
        snapshot_id: uuid::Uuid::new_v4().hyphenated().to_string(),
        snapshot_sha256: String::new(),
        platform: payload.platform,
        display_layout,
        windows: payload.windows,
        excluded_window_count: payload.excluded_window_count,
        truncated: payload.truncated,
    };
    snapshot.snapshot_sha256 = window_layout_sha256(&snapshot)?;
    snapshot.validate()?;
    store_window_layout_snapshot(snapshot.clone())?;
    Ok(json!({
        "text": format!(
            "Captured a short-lived opaque layout snapshot for {} ordinary {} windows. The snapshot remains only in volatile Local Connector memory.",
            snapshot.windows.len(), snapshot.platform
        ),
        "_structured_result": {
            "success": true,
            "mode": "read_only",
            "capture_scope": "ordinary_window_layout",
            "platform": snapshot.platform,
            "snapshot_id": snapshot.snapshot_id,
            "snapshot_sha256": snapshot.snapshot_sha256,
            "window_count": snapshot.windows.len(),
            "applications": window_layout_application_summary(&snapshot.windows),
            "excluded_window_count": snapshot.excluded_window_count,
            "truncated": snapshot.truncated,
            "maximum_windows": MAX_WINDOW_LAYOUT_WINDOWS,
            "expires_in_seconds": WINDOW_LAYOUT_SNAPSHOT_TTL.as_secs(),
            "persisted": false,
            "ordinary_visible_normal_windows_only": true,
            "model_supplied_window_identities_or_coordinates": false,
        }
    }))
}

fn validate_window_layout_snapshot_for_approval(snapshot: &WindowLayoutSnapshot) -> Result<()> {
    snapshot.validate()?;
    if active_display_layout_guard()? != snapshot.display_layout {
        return Err(anyhow!(
            "active display identity or geometry changed after layout capture; capture a new snapshot"
        ));
    }
    preflight_window_layout_snapshot(snapshot)
}

fn validate_approved_window_layout_snapshot(snapshot: &WindowLayoutSnapshot) -> Result<()> {
    snapshot.validate()?;
    if active_display_layout_guard()? != snapshot.display_layout {
        return Err(anyhow!(
            "active display identity or geometry changed after layout restore approval; capture and approve again"
        ));
    }
    Ok(())
}

fn required_bounded_i32(arguments: &Value, field: &str, minimum: i64, maximum: i64) -> Result<i32> {
    let value = arguments
        .get(field)
        .and_then(Value::as_i64)
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or_else(|| anyhow!("{field} must be an integer between {minimum} and {maximum}"))?;
    i32::try_from(value).map_err(|_| anyhow!("{field} is outside the supported integer range"))
}

fn active_display_layout_guard() -> Result<Vec<ApprovedDisplayGuard>> {
    active_displays().map(|displays| {
        displays
            .iter()
            .map(ApprovedDisplayGuard::from)
            .collect::<Vec<_>>()
    })
}

fn validate_requested_window_bounds_against_layout(
    request: &WindowBoundsRequest,
    display_layout: &[ApprovedDisplayGuard],
) -> Result<()> {
    let requested_left = f64::from(request.x);
    let requested_top = f64::from(request.y);
    let requested_right = requested_left + f64::from(request.width);
    let requested_bottom = requested_top + f64::from(request.height);
    let visible = display_layout.iter().any(|display| {
        let overlap_width = requested_right.min(display.origin_x + display.width)
            - requested_left.max(display.origin_x);
        let overlap_height = requested_bottom.min(display.origin_y + display.height)
            - requested_top.max(display.origin_y);
        overlap_width >= MIN_WINDOW_DIMENSION as f64
            && overlap_height >= MIN_WINDOW_DIMENSION as f64
    });
    if !visible {
        return Err(anyhow!(
            "requested window bounds must leave at least {MIN_WINDOW_DIMENSION} x {MIN_WINDOW_DIMENSION} desktop units visible on one active display"
        ));
    }
    Ok(())
}

fn window_display_layout_approval_argument(
    display_layout: &[ApprovedDisplayGuard],
) -> Result<String> {
    if display_layout.is_empty() || display_layout.len() > MAX_ACTIVE_DISPLAYS {
        return Err(anyhow!("approved active display layout is invalid"));
    }
    Ok(format!(
        "--display-layout-json={}",
        serde_json::to_string(display_layout)?
    ))
}

fn approved_window_display_layout(
    approved_command_args: Option<&[String]>,
) -> Result<Vec<ApprovedDisplayGuard>> {
    let arguments = approved_command_args
        .ok_or_else(|| anyhow!("approved active display layout is missing"))?;
    let encoded = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--display-layout-json="))
        .ok_or_else(|| anyhow!("approved active display layout is missing"))?;
    let layout = serde_json::from_str::<Vec<ApprovedDisplayGuard>>(encoded)
        .context("decode approved active display layout")?;
    if layout.is_empty() || layout.len() > MAX_ACTIVE_DISPLAYS {
        return Err(anyhow!("approved active display layout is invalid"));
    }
    Ok(layout)
}

fn validate_approved_window_display_layout(
    request: &WindowBoundsRequest,
    approved_command_args: Option<&[String]>,
) -> Result<()> {
    let approved = approved_window_display_layout(approved_command_args)?;
    let current = active_display_layout_guard()?;
    if current != approved {
        return Err(anyhow!(
            "active display identity or geometry changed after window-bounds approval; observe and approve again"
        ));
    }
    validate_requested_window_bounds_against_layout(request, &current)
}

fn validate_window_bounds_capability(target: &ApprovedFrontmostWindowGuard) -> Result<()> {
    target.validate()?;
    if !target.position_settable || !target.size_settable {
        return Err(anyhow!(
            "the current frontmost window does not expose writable position and size"
        ));
    }
    if target.fullscreen == Some(true) {
        return Err(anyhow!(
            "exit fullscreen before moving or resizing the frontmost macOS window"
        ));
    }
    if target.maximized == Some(true) {
        return Err(anyhow!(
            "restore the Windows foreground window before moving or resizing it"
        ));
    }
    Ok(())
}

fn validate_window_fullscreen_capability(
    target: &ApprovedFrontmostWindowGuard,
    requested: bool,
) -> Result<()> {
    target.validate()?;
    if target.platform != "macos" {
        return Err(anyhow!(
            "native frontmost-window fullscreen control is available only on macOS"
        ));
    }
    if !target.fullscreen_settable {
        return Err(anyhow!(
            "the current frontmost macOS window does not expose writable AXFullScreen state"
        ));
    }
    if target.fullscreen == Some(requested) {
        return Err(anyhow!(
            "the current frontmost macOS window is already in the requested fullscreen state"
        ));
    }
    Ok(())
}

fn validate_window_maximized_capability(
    target: &ApprovedFrontmostWindowGuard,
    requested: bool,
) -> Result<()> {
    target.validate()?;
    if target.platform != "windows" {
        return Err(anyhow!(
            "frontmost-window maximize control is available only on Windows"
        ));
    }
    if target.maximized == Some(requested) {
        return Err(anyhow!(
            "the current Windows foreground window is already in the requested maximized state"
        ));
    }
    Ok(())
}

fn window_approval_argument(target: &ApprovedFrontmostWindowGuard) -> Result<String> {
    target.validate()?;
    Ok(format!("--window-json={}", serde_json::to_string(target)?))
}

fn approved_window_guard(
    approved_command_args: Option<&[String]>,
) -> Result<ApprovedFrontmostWindowGuard> {
    let arguments = approved_command_args
        .ok_or_else(|| anyhow!("approved frontmost window identity is missing"))?;
    let encoded = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--window-json="))
        .ok_or_else(|| anyhow!("approved frontmost window identity is missing"))?;
    let target = serde_json::from_str::<ApprovedFrontmostWindowGuard>(encoded)
        .context("decode approved frontmost window identity")?;
    target.validate()?;
    Ok(target)
}

fn required_display_index(arguments: &Value) -> Result<u32> {
    reject_unknown_fields(arguments, &["display_index"])?;
    let index = arguments
        .get("display_index")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("display_index is required"))?;
    if index == 0 || index > MAX_ACTIVE_DISPLAYS as u64 {
        return Err(anyhow!(
            "display_index must be between 1 and {MAX_ACTIVE_DISPLAYS}"
        ));
    }
    Ok(index as u32)
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

#[cfg(target_os = "macos")]
fn active_displays() -> Result<Vec<DisplayTarget>> {
    let mut display_ids = [0_u32; MAX_ACTIVE_DISPLAYS];
    let mut count = 0_u32;
    // SAFETY: CoreGraphics writes at most MAX_ACTIVE_DISPLAYS IDs into the fixed-size buffer and
    // writes one count value. All returned display IDs are value types.
    let status = unsafe {
        CGGetActiveDisplayList(
            MAX_ACTIVE_DISPLAYS as u32,
            display_ids.as_mut_ptr(),
            &mut count,
        )
    };
    if status != 0 {
        return Err(anyhow!("macOS active display discovery failed: {status}"));
    }
    if count == 0 || count as usize > MAX_ACTIVE_DISPLAYS {
        return Err(anyhow!("macOS reported no usable active displays"));
    }
    // SAFETY: These CoreGraphics display queries take and return value types only.
    let main_display_id = unsafe { CGMainDisplayID() };
    let mut displays = Vec::with_capacity(count as usize);
    for display_id in display_ids[..count as usize].iter().copied() {
        // SAFETY: display_id came from CGGetActiveDisplayList and each query returns a value type.
        let bounds = unsafe { CGDisplayBounds(display_id) };
        let pixels_wide = unsafe { CGDisplayPixelsWide(display_id) };
        let pixels_high = unsafe { CGDisplayPixelsHigh(display_id) };
        let rotation_degrees = unsafe { CGDisplayRotation(display_id) };
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return Err(anyhow!("macOS returned invalid active display bounds"));
        }
        displays.push(DisplayTarget {
            index: 0,
            display_id,
            is_main: display_id == main_display_id,
            origin_x: bounds.origin.x,
            origin_y: bounds.origin.y,
            width: bounds.size.width,
            height: bounds.size.height,
            pixels_wide,
            pixels_high,
            rotation_degrees,
        });
    }
    displays.sort_by_key(|display| !display.is_main);
    if !displays.first().is_some_and(|display| display.is_main) {
        return Err(anyhow!("macOS main display is unavailable"));
    }
    for (offset, display) in displays.iter_mut().enumerate() {
        display.index = (offset + 1) as u32;
    }
    Ok(displays)
}

#[cfg(target_os = "windows")]
fn active_displays() -> Result<Vec<DisplayTarget>> {
    windows::active_displays()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn active_displays() -> Result<Vec<DisplayTarget>> {
    Err(anyhow!(
        "Computer Use display discovery is unsupported on this platform"
    ))
}

fn resolve_display(index: Option<&Value>) -> Result<DisplayTarget> {
    let displays = active_displays()?;
    if let Some(index) = index {
        let index = index
            .as_u64()
            .ok_or_else(|| anyhow!("display_index must be an integer"))?;
        if index == 0 || index > MAX_ACTIVE_DISPLAYS as u64 {
            return Err(anyhow!(
                "display_index must be between 1 and {MAX_ACTIVE_DISPLAYS}"
            ));
        }
        return displays
            .into_iter()
            .find(|display| display.index == index as u32)
            .ok_or_else(|| anyhow!("the selected display is no longer active"));
    }
    displays
        .into_iter()
        .find(|display| display.is_main)
        .ok_or_else(|| anyhow!("the main display is unavailable"))
}

fn list_displays() -> Result<Value> {
    let displays = active_displays()?;
    let rows = displays
        .iter()
        .map(|display| {
            let scale_x = display.pixels_wide as f64 / display.width;
            let scale_y = display.pixels_high as f64 / display.height;
            json!({
                "display_index": display.index,
                "display_id": display.display_id,
                "is_main": display.is_main,
                "bounds_points": {
                    "x": display.origin_x,
                    "y": display.origin_y,
                    "width": display.width,
                    "height": display.height,
                },
                "pixels": {"width": display.pixels_wide, "height": display.pixels_high},
                "scale": {"x": scale_x, "y": scale_y},
                "rotation_degrees": display.rotation_degrees,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "success": true,
        "mode": "read_only",
        "platform": current_platform_name(),
        "display_count": rows.len(),
        "displays": rows,
        "hotplug_sensitive": true,
    }))
}

fn current_platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unsupported"
    }
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

#[cfg(target_os = "macos")]
struct CoreGraphicsUpGuard {
    event: *mut c_void,
}

#[cfg(target_os = "macos")]
impl CoreGraphicsUpGuard {
    fn new(event: *mut c_void) -> Self {
        Self { event }
    }

    fn set_location(&mut self, point: CGPoint) {
        // SAFETY: event is retained and owned by this guard until release or drop.
        unsafe { CGEventSetLocation(self.event, point) };
    }

    fn release(mut self) {
        self.post_and_release();
    }

    fn post_and_release(&mut self) {
        if self.event.is_null() {
            return;
        }
        // SAFETY: event is a retained CoreGraphics event owned by this guard. It is posted and
        // released exactly once, then cleared so Drop is idempotent.
        unsafe {
            CGEventPost(0, self.event);
            CFRelease(self.event);
        }
        self.event = std::ptr::null_mut();
    }
}

#[cfg(target_os = "macos")]
impl Drop for CoreGraphicsUpGuard {
    fn drop(&mut self) {
        self.post_and_release();
    }
}

#[cfg(target_os = "macos")]
fn click(action: ClickAction<'_>, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
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
        // SAFETY: CoreGraphics accepts a null source, copies the point by value, and returns
        // retained events. The up event is armed before down is posted, so every later unwind or
        // return path posts an up event before releasing it.
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

fn click_result(action: &ClickAction<'_>) -> Value {
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
fn drag(action: DragAction, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
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
    // SAFETY: CoreGraphics accepts a null source and copies both points by value. The down event
    // is released immediately after posting. The up event is transferred to MouseUpGuard before
    // any mouse-down is posted, so every later return path posts and releases mouse-up exactly once.
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
    // SAFETY: down is a retained event returned above, posted synchronously, then released once.
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
        // SAFETY: CoreGraphics copies the point and returns a retained event, which is posted and
        // released exactly once below. MouseUpGuard keeps the release event at the last posted point.
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
fn click(action: ClickAction<'_>, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    windows::click(action, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn click(_action: ClickAction<'_>, _action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}

#[cfg(target_os = "windows")]
fn drag(action: DragAction, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    windows::drag(action, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn drag(_action: DragAction, _action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn press_key(action: KeyAction<'_>) -> Result<Value> {
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
    // SAFETY: down is posted synchronously and released exactly once. key_up remains armed across
    // every later unwind path.
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

#[cfg(target_os = "macos")]
fn type_text(action: TypedTextAction<'_>) -> Result<Value> {
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
    // event is armed first so failures never leave the generated key logically held.
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
        Value::String(target.class.as_str().to_string()),
    );
    Ok(result)
}

fn typed_text_result(action: &TypedTextAction<'_>) -> Value {
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
fn type_text(action: TypedTextAction<'_>) -> Result<Value> {
    windows::type_text(action)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn type_text(_action: TypedTextAction<'_>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use secure-field-aware text input is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
const AX_ERROR_SUCCESS: i32 = 0;
#[cfg(target_os = "macos")]
const AX_ERROR_ATTRIBUTE_UNSUPPORTED: i32 = -25205;
#[cfg(target_os = "macos")]
const AX_ERROR_NO_VALUE: i32 = -25212;
#[cfg(target_os = "macos")]
const AX_VALUE_TYPE_CGPOINT: u32 = 1;
#[cfg(target_os = "macos")]
const AX_VALUE_TYPE_CGSIZE: u32 = 2;
#[cfg(target_os = "macos")]
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct MacCfObject(*const c_void);

#[cfg(target_os = "macos")]
impl MacCfObject {
    fn from_owned(value: *const c_void, label: &str) -> Result<Self> {
        if value.is_null() {
            return Err(anyhow!("macOS Accessibility returned no {label}"));
        }
        Ok(Self(value))
    }

    fn string(value: &str) -> Result<Self> {
        let byte_count = isize::try_from(value.len())
            .map_err(|_| anyhow!("macOS Accessibility attribute name is too long"))?;
        // SAFETY: CoreFoundation copies the bounded UTF-8 bytes into one retained CFString.
        let string = unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                value.as_bytes().as_ptr(),
                byte_count,
                CF_STRING_ENCODING_UTF8,
                0,
            )
        };
        Self::from_owned(string, "CoreFoundation string")
    }

    fn as_ptr(&self) -> *const c_void {
        self.0
    }
}

#[cfg(target_os = "macos")]
impl Clone for MacCfObject {
    fn clone(&self) -> Self {
        // SAFETY: self owns a live CoreFoundation reference, and the clone balances this retain in
        // its Drop implementation.
        unsafe {
            CFRetain(self.0);
        }
        Self(self.0)
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacCfObject {
    fn drop(&mut self) {
        // SAFETY: every MacCfObject is created from a retained CoreFoundation result or CFRetain
        // and is released exactly once here.
        unsafe {
            CFRelease(self.0);
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacTextTargetClass {
    NativeTextControl,
    ContentEditable,
}

#[cfg(target_os = "macos")]
impl MacTextTargetClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::NativeTextControl => "native_text_control",
            Self::ContentEditable => "contenteditable",
        }
    }
}

#[cfg(target_os = "macos")]
fn classify_macos_text_target(
    native_role: bool,
    rich_text_role: bool,
    explicitly_editable: bool,
    value_settable: bool,
    selection_range_settable: bool,
) -> Result<MacTextTargetClass> {
    if native_role {
        if value_settable || selection_range_settable {
            return Ok(MacTextTargetClass::NativeTextControl);
        }
        return Err(anyhow!(
            "Computer Use refuses to type into a read-only macOS text control"
        ));
    }
    if rich_text_role && explicitly_editable && selection_range_settable {
        return Ok(MacTextTargetClass::ContentEditable);
    }
    Err(anyhow!(
        "Computer Use text input requires a focused writable native text control or explicit contenteditable target"
    ))
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct ValidatedMacTextTarget {
    application: MacCfObject,
    focused: MacCfObject,
    target: MacCfObject,
    pid: i32,
    class: MacTextTargetClass,
}

#[cfg(target_os = "macos")]
impl ValidatedMacTextTarget {
    fn validate() -> Result<Self> {
        // SAFETY: AXUIElementCreateSystemWide returns one retained process-local accessibility
        // object. The wrapper releases it after all bounded synchronous queries complete.
        let system = MacCfObject::from_owned(
            unsafe { AXUIElementCreateSystemWide() },
            "system-wide Accessibility element",
        )?;
        // SAFETY: this limits AX messaging only inside the one-shot helper process. No input has
        // been posted at this point, and failure aborts before target inspection.
        let timeout_status = unsafe { AXUIElementSetMessagingTimeout(system.as_ptr(), 2.0) };
        if timeout_status != AX_ERROR_SUCCESS {
            return Err(anyhow!(
                "macOS Accessibility messaging timeout setup failed: {timeout_status}"
            ));
        }
        let application = required_ax_element(&system, "AXFocusedApplication")?;
        if !required_ax_bool(&application, "AXFrontmost")? {
            return Err(anyhow!(
                "Computer Use text input requires the focused macOS application to remain frontmost"
            ));
        }
        let pid = ax_element_pid(&application)?;
        if pid <= 0 {
            return Err(anyhow!(
                "macOS Accessibility returned an invalid foreground process identity"
            ));
        }
        let focused = required_ax_element(&application, "AXFocusedUIElement")?;
        if ax_element_pid(&focused)? != pid {
            return Err(anyhow!(
                "macOS focused text element does not belong to the frontmost application"
            ));
        }
        if !required_ax_bool(&focused, "AXEnabled")? || !required_ax_bool(&focused, "AXFocused")? {
            return Err(anyhow!(
                "Computer Use text input requires an enabled, keyboard-focused macOS control"
            ));
        }
        ensure_macos_text_element_is_not_secure(&focused)?;

        let focused_native = ax_string_matches(
            &focused,
            "AXRole",
            &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"],
        )?;
        let focused_editable = optional_ax_bool(&focused, "AXIsEditable")? == Some(true);
        let target = if focused_native || focused_editable {
            focused.clone()
        } else {
            optional_ax_element(&focused, "AXEditableAncestor")?
                .or(optional_ax_element(&focused, "AXHighestEditableAncestor")?)
                .ok_or_else(|| {
                    anyhow!("Computer Use text input requires an explicit macOS editable target")
                })?
        };
        if ax_element_pid(&target)? != pid {
            return Err(anyhow!(
                "macOS editable target does not belong to the frontmost application"
            ));
        }
        if !required_ax_bool(&target, "AXEnabled")? {
            return Err(anyhow!(
                "Computer Use text input requires an enabled macOS editable target"
            ));
        }
        ensure_macos_text_element_is_not_secure(&target)?;
        ensure_nonempty_ax_bounds(&target)?;

        let native_role = ax_string_matches(
            &target,
            "AXRole",
            &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"],
        )?;
        let rich_text_role =
            ax_string_matches(&target, "AXRole", &["AXWebArea", "AXGroup", "AXStaticText"])?;
        let explicitly_editable = optional_ax_bool(&target, "AXIsEditable")? == Some(true);
        let value_settable = optional_ax_attribute_settable(&target, "AXValue")? == Some(true);
        let selection_range_settable =
            optional_ax_attribute_settable(&target, "AXSelectedTextRange")? == Some(true);
        let class = classify_macos_text_target(
            native_role,
            rich_text_role,
            explicitly_editable,
            value_settable,
            selection_range_settable,
        )?;
        Ok(Self {
            application,
            focused,
            target,
            pid,
            class,
        })
    }

    fn ensure_still_focused(&self) -> Result<()> {
        let current = Self::validate()?;
        if current.pid != self.pid
            || current.class != self.class
            || !cf_equal(&current.application, &self.application)
            || !cf_equal(&current.focused, &self.focused)
            || !cf_equal(&current.target, &self.target)
        {
            return Err(anyhow!(
                "macOS focused editable target changed after validation"
            ));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn ax_copy_attribute(element: &MacCfObject, attribute: &str) -> Result<Option<MacCfObject>> {
    let attribute_name = MacCfObject::string(attribute)?;
    let mut value = std::ptr::null();
    // SAFETY: element and attribute_name are live CoreFoundation objects. On success the API
    // writes one retained value, which is immediately transferred into MacCfObject ownership.
    let status = unsafe {
        AXUIElementCopyAttributeValue(element.as_ptr(), attribute_name.as_ptr(), &mut value)
    };
    match status {
        AX_ERROR_SUCCESS => MacCfObject::from_owned(value, attribute).map(Some),
        AX_ERROR_ATTRIBUTE_UNSUPPORTED | AX_ERROR_NO_VALUE => Ok(None),
        _ => Err(anyhow!(
            "macOS Accessibility could not read {attribute}: {status}"
        )),
    }
}

#[cfg(target_os = "macos")]
fn required_ax_element(element: &MacCfObject, attribute: &str) -> Result<MacCfObject> {
    optional_ax_element(element, attribute)?
        .ok_or_else(|| anyhow!("macOS Accessibility did not provide required {attribute} identity"))
}

#[cfg(target_os = "macos")]
fn optional_ax_element(element: &MacCfObject, attribute: &str) -> Result<Option<MacCfObject>> {
    let Some(value) = ax_copy_attribute(element, attribute)? else {
        return Ok(None);
    };
    // SAFETY: value owns a live CoreFoundation object for the duration of this type query.
    let value_type = unsafe { CFGetTypeID(value.as_ptr()) };
    // SAFETY: AXUIElementGetTypeID returns a process-stable numeric type identifier.
    if value_type != unsafe { AXUIElementGetTypeID() } {
        return Err(anyhow!(
            "macOS Accessibility returned an invalid {attribute} element type"
        ));
    }
    Ok(Some(value))
}

#[cfg(target_os = "macos")]
fn required_ax_bool(element: &MacCfObject, attribute: &str) -> Result<bool> {
    optional_ax_bool(element, attribute)?
        .ok_or_else(|| anyhow!("macOS Accessibility did not provide required {attribute} state"))
}

#[cfg(target_os = "macos")]
fn optional_ax_bool(element: &MacCfObject, attribute: &str) -> Result<Option<bool>> {
    let Some(value) = ax_copy_attribute(element, attribute)? else {
        return Ok(None);
    };
    // SAFETY: value owns a live CoreFoundation object for both type and boolean queries.
    if unsafe { CFGetTypeID(value.as_ptr()) } != unsafe { CFBooleanGetTypeID() } {
        return Err(anyhow!(
            "macOS Accessibility returned an invalid {attribute} boolean type"
        ));
    }
    // SAFETY: the type identity above proves value is a CFBoolean.
    Ok(Some(unsafe { CFBooleanGetValue(value.as_ptr()) } != 0))
}

#[cfg(target_os = "macos")]
fn optional_ax_attribute_settable(element: &MacCfObject, attribute: &str) -> Result<Option<bool>> {
    let attribute_name = MacCfObject::string(attribute)?;
    let mut settable = 0_u8;
    // SAFETY: element and attribute_name are live accessibility/CoreFoundation objects and the API
    // writes only one Boolean into settable.
    let status = unsafe {
        AXUIElementIsAttributeSettable(element.as_ptr(), attribute_name.as_ptr(), &mut settable)
    };
    match status {
        AX_ERROR_SUCCESS => Ok(Some(settable != 0)),
        AX_ERROR_ATTRIBUTE_UNSUPPORTED | AX_ERROR_NO_VALUE => Ok(None),
        _ => Err(anyhow!(
            "macOS Accessibility could not confirm whether {attribute} is writable: {status}"
        )),
    }
}

#[cfg(target_os = "macos")]
fn ax_string_matches(element: &MacCfObject, attribute: &str, expected: &[&str]) -> Result<bool> {
    let Some(value) = ax_copy_attribute(element, attribute)? else {
        return Ok(false);
    };
    // SAFETY: value owns a live CoreFoundation object for this type query.
    if unsafe { CFGetTypeID(value.as_ptr()) } != unsafe { CFStringGetTypeID() } {
        return Err(anyhow!(
            "macOS Accessibility returned an invalid {attribute} string type"
        ));
    }
    for candidate in expected {
        let candidate = MacCfObject::string(candidate)?;
        if cf_equal(&value, &candidate) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn ax_element_pid(element: &MacCfObject) -> Result<i32> {
    let mut pid = 0_i32;
    // SAFETY: element is a validated AXUIElement and pid points to one writable process id.
    let status = unsafe { AXUIElementGetPid(element.as_ptr(), &mut pid) };
    if status != AX_ERROR_SUCCESS {
        return Err(anyhow!(
            "macOS Accessibility could not read process identity: {status}"
        ));
    }
    Ok(pid)
}

#[cfg(target_os = "macos")]
fn ensure_macos_text_element_is_not_secure(element: &MacCfObject) -> Result<()> {
    let secure_role = ax_string_matches(
        element,
        "AXRole",
        &[
            "AXSecureTextField",
            "AXPasswordField",
            "AXPasswordTextField",
        ],
    )? || ax_string_matches(
        element,
        "AXSubrole",
        &[
            "AXSecureTextField",
            "AXPasswordField",
            "AXPasswordTextField",
        ],
    )?;
    let protected = optional_ax_bool(element, "AXContainsProtectedContent")? == Some(true);
    if secure_role || protected {
        return Err(anyhow!(
            "Computer Use refuses to type into a secure, password, or protected macOS field"
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn ensure_nonempty_ax_bounds(element: &MacCfObject) -> Result<()> {
    let position = required_ax_value(element, "AXPosition", AX_VALUE_TYPE_CGPOINT)?;
    let size = required_ax_value(element, "AXSize", AX_VALUE_TYPE_CGSIZE)?;
    let mut point = CGPoint { x: 0.0, y: 0.0 };
    let mut dimensions = CGSize {
        width: 0.0,
        height: 0.0,
    };
    // SAFETY: both retained values were type-checked for the exact destination structures, whose
    // pointers remain valid for these synchronous copies.
    let point_ok = unsafe {
        AXValueGetValue(
            position.as_ptr(),
            AX_VALUE_TYPE_CGPOINT,
            (&mut point as *mut CGPoint).cast(),
        )
    } != 0;
    let size_ok = unsafe {
        AXValueGetValue(
            size.as_ptr(),
            AX_VALUE_TYPE_CGSIZE,
            (&mut dimensions as *mut CGSize).cast(),
        )
    } != 0;
    if !point_ok
        || !size_ok
        || !point.x.is_finite()
        || !point.y.is_finite()
        || !dimensions.width.is_finite()
        || !dimensions.height.is_finite()
        || dimensions.width <= 0.0
        || dimensions.height <= 0.0
    {
        return Err(anyhow!(
            "Computer Use text input requires a visible macOS editable target with non-empty bounds"
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn required_ax_value(
    element: &MacCfObject,
    attribute: &str,
    expected_value_type: u32,
) -> Result<MacCfObject> {
    let value = ax_copy_attribute(element, attribute)?.ok_or_else(|| {
        anyhow!("macOS Accessibility did not provide required {attribute} bounds")
    })?;
    // SAFETY: value owns a live CoreFoundation object for both type queries.
    if unsafe { CFGetTypeID(value.as_ptr()) } != unsafe { AXValueGetTypeID() }
        || unsafe { AXValueGetType(value.as_ptr()) } != expected_value_type
    {
        return Err(anyhow!(
            "macOS Accessibility returned invalid {attribute} bounds"
        ));
    }
    Ok(value)
}

#[cfg(target_os = "macos")]
fn cf_equal(left: &MacCfObject, right: &MacCfObject) -> bool {
    // SAFETY: both arguments own live CoreFoundation objects for the duration of the comparison.
    unsafe { CFEqual(left.as_ptr(), right.as_ptr()) != 0 }
}

#[cfg(target_os = "macos")]
fn scroll(action: ScrollAction) -> Result<Value> {
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
fn scroll(action: ScrollAction) -> Result<Value> {
    windows::scroll(action)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn scroll(_action: ScrollAction) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn lookup_application(pid: u32) -> Result<Value> {
    execute_jxa(LOOKUP_APPLICATION_JXA, &[pid.to_string()])
}

#[cfg(target_os = "windows")]
fn lookup_application(pid: u32) -> Result<Value> {
    windows::lookup_application(pid)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn lookup_application(_pid: u32) -> Result<Value> {
    Err(anyhow!(
        "Computer Use application discovery is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    helper::frontmost_window_control_target()
}

#[cfg(target_os = "windows")]
fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    windows::frontmost_window_control_target()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    Err(anyhow!(
        "Computer Use frontmost-window control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard> {
    macos_frontmost_window_control_target_local()
}

#[cfg(target_os = "windows")]
fn frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard> {
    windows::frontmost_window_control_target()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard> {
    Err(anyhow!(
        "Computer Use frontmost-window control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn restore_window_layout(
    snapshot: &WindowLayoutSnapshot,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    snapshot.validate()?;
    ensure_action_not_cancelled(action_cancelled)?;
    let snapshot_json = serde_json::to_string(snapshot)?;
    let mut result = execute_jxa_action(
        RESTORE_WINDOW_LAYOUT_JXA,
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
fn restore_window_layout(
    snapshot: &WindowLayoutSnapshot,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    windows::restore_window_layout(snapshot, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn restore_window_layout(
    _snapshot: &WindowLayoutSnapshot,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "Computer Use window layout restore is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn set_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    set_macos_frontmost_window_bounds(request, approved, action_cancelled)
}

#[cfg(target_os = "windows")]
fn set_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    windows::set_frontmost_window_bounds(request, approved, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set_frontmost_window_bounds(
    _request: WindowBoundsRequest,
    _approved: ApprovedFrontmostWindowGuard,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "Computer Use frontmost-window bounds control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn rollback_frontmost_window_bounds(
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
fn rollback_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    windows::rollback_frontmost_window_bounds(request, approved)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn rollback_frontmost_window_bounds(
    _request: WindowBoundsRequest,
    _approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    json!({"attempted": false, "restored": false, "reason": "platform_restore_unavailable"})
}

#[cfg(target_os = "macos")]
fn set_frontmost_window_fullscreen(
    fullscreen: bool,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    set_macos_frontmost_window_fullscreen(fullscreen, approved, action_cancelled)
}

#[cfg(not(target_os = "macos"))]
fn set_frontmost_window_fullscreen(
    _fullscreen: bool,
    _approved: ApprovedFrontmostWindowGuard,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "native frontmost-window fullscreen control is available only on macOS"
    ))
}

#[cfg(target_os = "macos")]
fn rollback_frontmost_window_fullscreen(
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
fn rollback_frontmost_window_fullscreen(
    _fullscreen: bool,
    _approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    json!({"attempted": false, "restored": false, "reason": "platform_restore_unavailable"})
}

#[cfg(target_os = "windows")]
fn set_frontmost_window_maximized(
    maximized: bool,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    windows::set_frontmost_window_maximized(maximized, approved, action_cancelled)
}

#[cfg(not(target_os = "windows"))]
fn set_frontmost_window_maximized(
    _maximized: bool,
    _approved: ApprovedFrontmostWindowGuard,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    Err(anyhow!(
        "frontmost-window maximize control is available only on Windows"
    ))
}

#[cfg(target_os = "windows")]
fn rollback_frontmost_window_maximized(
    maximized: bool,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    windows::rollback_frontmost_window_maximized(maximized, approved)
}

#[cfg(not(target_os = "windows"))]
fn rollback_frontmost_window_maximized(
    _maximized: bool,
    _approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    json!({"attempted": false, "restored": false, "reason": "platform_restore_unavailable"})
}

fn approved_application_name(approved_command_args: Option<&[String]>) -> Result<String> {
    let arguments = approved_command_args.ok_or_else(|| {
        anyhow!("Computer Use application activation is missing approved identity context")
    })?;
    let encoded = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--application-json="))
        .ok_or_else(|| anyhow!("approved application identity is missing"))?;
    let application =
        serde_json::from_str::<String>(encoded).context("decode approved application identity")?;
    if application.is_empty() || application.chars().count() > 120 {
        return Err(anyhow!("approved application identity is invalid"));
    }
    Ok(application)
}

#[cfg(target_os = "macos")]
fn activate_application_with_rollback(
    pid: u32,
    approved_application: String,
    action_cancelled: Option<&AtomicBool>,
) -> Result<(Value, ApplicationActivationRollbackGuard)> {
    let previous = frontmost_application_identity()?;
    ensure_action_not_cancelled(action_cancelled)?;
    let mut result = execute_jxa(
        ACTIVATE_APPLICATION_JXA,
        &[
            pid.to_string(),
            approved_application.clone(),
            previous.pid.to_string(),
            previous.application.clone(),
        ],
    )?;
    let map = result
        .as_object_mut()
        .ok_or_else(|| anyhow!("Computer Use activation result must be an object"))?;
    map.insert(
        "mode".to_string(),
        Value::String("approved_input".to_string()),
    );
    map.insert(
        "action".to_string(),
        Value::String("activate_application".to_string()),
    );
    map.remove("sensitive_text_policy");
    Ok((
        result,
        ApplicationActivationRollbackGuard {
            changed_frontmost_application: previous.pid != pid,
            previous,
            target: ApplicationIdentity {
                pid,
                application: approved_application,
            },
        },
    ))
}

#[cfg(target_os = "windows")]
fn activate_application_with_rollback(
    pid: u32,
    approved_application: String,
    action_cancelled: Option<&AtomicBool>,
) -> Result<(Value, ApplicationActivationRollbackGuard)> {
    windows::activate_application_with_rollback(pid, approved_application, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn activate_application_with_rollback(
    _pid: u32,
    _approved_application: String,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<(Value, ApplicationActivationRollbackGuard)> {
    Err(anyhow!(
        "Computer Use application activation is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn frontmost_application_identity() -> Result<ApplicationIdentity> {
    let result = execute_jxa(FRONTMOST_APPLICATION_JXA, &[])?;
    let pid = result
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(|| anyhow!("macOS frontmost application PID is invalid"))?;
    let application = result
        .get("application")
        .and_then(Value::as_str)
        .filter(|application| !application.is_empty() && application.chars().count() <= 240)
        .ok_or_else(|| anyhow!("macOS frontmost application identity is invalid"))?
        .to_string();
    Ok(ApplicationIdentity { pid, application })
}

#[cfg(target_os = "macos")]
fn rollback_application_activation(guard: &ApplicationActivationRollbackGuard) -> Result<Value> {
    if !guard.changed_frontmost_application {
        return Ok(json!({
            "scope": "frontmost_application_activation_only",
            "rollback_on_in_flight_cancel": true,
            "attempted": false,
            "restored": true,
            "reason": "activation_did_not_change_frontmost_application",
            "previous_pid": guard.previous.pid,
            "target_pid": guard.target.pid,
            "application_content_rollback": false,
            "window_geometry_rollback": false,
        }));
    }
    let result = execute_jxa(
        RESTORE_APPLICATION_JXA,
        &[
            guard.previous.pid.to_string(),
            guard.previous.application.clone(),
            guard.target.pid.to_string(),
            guard.target.application.clone(),
        ],
    )?;
    normalize_application_rollback_result(result, guard.previous.pid, guard.target.pid)
}

#[cfg(target_os = "windows")]
fn rollback_application_activation(guard: &ApplicationActivationRollbackGuard) -> Result<Value> {
    windows::rollback_application_activation(guard)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn rollback_application_activation(_guard: &ApplicationActivationRollbackGuard) -> Result<Value> {
    Err(anyhow!(
        "Computer Use application activation rollback is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn normalize_application_rollback_result(
    result: Value,
    previous_pid: u32,
    target_pid: u32,
) -> Result<Value> {
    let attempted = result
        .get("attempted")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("application activation rollback result is missing attempted"))?;
    let restored = result
        .get("restored")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("application activation rollback result is missing restored"))?;
    let reason = result
        .get("reason")
        .and_then(Value::as_str)
        .filter(|reason| {
            matches!(
                *reason,
                "activation_did_not_change_frontmost_application"
                    | "foreground_changed_after_activation"
                    | "previous_application_identity_unavailable"
                    | "platform_refused_restore"
                    | "cancelled_activation_restored"
            )
        })
        .ok_or_else(|| anyhow!("application activation rollback result has an invalid reason"))?;
    Ok(json!({
        "scope": "frontmost_application_activation_only",
        "rollback_on_in_flight_cancel": true,
        "attempted": attempted,
        "restored": restored,
        "reason": reason,
        "previous_pid": previous_pid,
        "target_pid": target_pid,
        "application_content_rollback": false,
        "window_geometry_rollback": false,
    }))
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

#[cfg(target_os = "windows")]
fn press_key(action: KeyAction<'_>) -> Result<Value> {
    windows::press_key(action)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn press_key(_action: KeyAction<'_>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use input control is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn macos_frontmost_window_identity() -> Result<MacosFrontmostWindowIdentity> {
    let value = execute_jxa(FRONTMOST_WINDOW_CAPTURE_TARGET_JXA, &[])?;
    let identity = serde_json::from_value::<MacosFrontmostWindowIdentity>(value)
        .context("decode macOS frontmost window capture target")?;
    identity.validate()?;
    Ok(identity)
}

#[cfg(target_os = "macos")]
fn macos_frontmost_window_control_target_local() -> Result<ApprovedFrontmostWindowGuard> {
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

#[cfg(target_os = "macos")]
fn capture_frontmost_window() -> Result<Value> {
    let before = macos_frontmost_window_identity()?;
    let capture_arguments = vec!["-l".to_string(), before.window_id.to_string()];
    let bytes = capture_macos_jpeg(
        capture_arguments.as_slice(),
        format!("window-{}.jpg", before.window_id).as_str(),
    )?;
    let after = macos_frontmost_window_identity()?;
    if !before.same_identity_and_geometry(&after) {
        return Err(anyhow!(
            "macOS frontmost window identity or geometry changed during capture"
        ));
    }
    frontmost_window_screenshot_result(bytes.as_slice(), &before.capture_target())
}

#[cfg(target_os = "windows")]
fn capture_frontmost_window() -> Result<Value> {
    windows::capture_frontmost_window()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_frontmost_window() -> Result<Value> {
    Err(anyhow!(
        "Computer Use frontmost-window screenshots are unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn capture_macos_jpeg(capture_arguments: &[String], file_name: &str) -> Result<Vec<u8>> {
    if capture_arguments.len() > 4
        || capture_arguments.iter().any(|argument| {
            argument.is_empty()
                || argument.len() > 32
                || !argument
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
        || file_name.is_empty()
        || file_name.len() > 64
        || !file_name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.'))
    {
        return Err(anyhow!("macOS screenshot target arguments are invalid"));
    }
    let screenshot_dir = tempfile::Builder::new()
        .prefix("chatos-computer-use-")
        .tempdir()
        .context("create private Computer Use screenshot directory")?;
    let screenshot_path = screenshot_dir.path().join(file_name);
    let mut command = Command::new(MACOS_SCREENCAPTURE_PATH);
    command
        .arg("-x")
        .args(capture_arguments)
        .args(["-t", "jpg"])
        .arg(screenshot_path.as_path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .context("start macOS Computer Use screenshot capture")?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Computer Use screenshot stderr is unavailable"))?;
    let stderr_reader = thread::spawn(move || {
        read_limited(stderr, "screenshot stderr", COMPUTER_USE_STDERR_MAX_BYTES)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().context("poll Computer Use screenshot")? {
            break status;
        }
        if started.elapsed() >= COMPUTER_USE_COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_reader.join();
            return Err(anyhow!(
                "Computer Use screenshot timed out after {} seconds",
                COMPUTER_USE_COMMAND_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(25));
    };
    let stderr = join_reader(stderr_reader, "screenshot stderr")?;
    if !status.success() {
        return Err(classify_macos_screenshot_error(
            String::from_utf8_lossy(stderr.as_slice()).trim(),
            status,
        ));
    }
    let metadata =
        fs::metadata(screenshot_path.as_path()).context("read Computer Use screenshot metadata")?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() as usize > COMPUTER_USE_SCREENSHOT_MAX_BYTES
    {
        return Err(anyhow!(
            "Computer Use screenshot is empty or exceeds {} bytes",
            COMPUTER_USE_SCREENSHOT_MAX_BYTES
        ));
    }
    fs::read(screenshot_path.as_path()).context("read Computer Use screenshot")
}

#[cfg(target_os = "macos")]
fn capture_display(requested_index: Option<u32>) -> Result<Value> {
    let display = if let Some(index) = requested_index {
        active_displays()?
            .into_iter()
            .find(|display| display.index == index)
            .ok_or_else(|| anyhow!("the selected display is no longer active"))?
    } else {
        resolve_display(None)?
    };
    let capture_arguments = if requested_index.is_some() {
        vec!["-D".to_string(), display.index.to_string()]
    } else {
        vec!["-m".to_string()]
    };
    let file_name = format!("display-{}.jpg", display.index);
    let bytes = capture_macos_jpeg(capture_arguments.as_slice(), file_name.as_str())?;
    screenshot_result(bytes.as_slice(), &display)
}

#[cfg(target_os = "windows")]
fn capture_display(requested_index: Option<u32>) -> Result<Value> {
    let display = if let Some(index) = requested_index {
        active_displays()?
            .into_iter()
            .find(|display| display.index == index)
            .ok_or_else(|| anyhow!("the selected display is no longer active"))?
    } else {
        resolve_display(None)?
    };
    windows::capture_display(&display)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_display(_requested_index: Option<u32>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use screenshots are unsupported on this platform"
    ))
}

fn screenshot_result(bytes: &[u8], display: &DisplayTarget) -> Result<Value> {
    let (mime_type, prefix) = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        ("image/jpeg", "data:image/jpeg;base64,")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        ("image/png", "data:image/png;base64,")
    } else {
        return Err(anyhow!(
            "Computer Use screenshot has an unsupported image format"
        ));
    };
    if bytes.len() > COMPUTER_USE_SCREENSHOT_MAX_BYTES {
        return Err(anyhow!(
            "Computer Use screenshot exceeds the {} byte limit",
            COMPUTER_USE_SCREENSHOT_MAX_BYTES
        ));
    }
    let sha256 = hex::encode(Sha256::digest(bytes));
    let image_url = format!("{prefix}{}", STANDARD.encode(bytes));
    Ok(json!({
        "text": format!("Captured a read-only screenshot of {} display {} and attached it as transient image input for the next model step.", current_platform_name(), display.index),
        "_structured_result": {
            "success": true,
            "mode": "read_only",
            "capture_scope": if display.is_main { "main_display" } else { "selected_display" },
            "display_index": display.index,
            "display_id": display.display_id,
            "is_main": display.is_main,
            "mime_type": mime_type,
            "size_bytes": bytes.len(),
            "sha256": sha256,
            "persisted": false,
            "sensitive_content_possible": true
        },
        "_model_input": [{
            "type": "input_image",
            "image_url": image_url,
            "detail": "high"
        }]
    }))
}

fn frontmost_window_screenshot_result(
    bytes: &[u8],
    target: &FrontmostWindowCaptureTarget,
) -> Result<Value> {
    let (mime_type, prefix) = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        ("image/jpeg", "data:image/jpeg;base64,")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        ("image/png", "data:image/png;base64,")
    } else {
        return Err(anyhow!(
            "Computer Use frontmost-window screenshot has an unsupported image format"
        ));
    };
    if bytes.is_empty() || bytes.len() > COMPUTER_USE_SCREENSHOT_MAX_BYTES {
        return Err(anyhow!(
            "Computer Use frontmost-window screenshot exceeds the {} byte limit",
            COMPUTER_USE_SCREENSHOT_MAX_BYTES
        ));
    }
    let sha256 = hex::encode(Sha256::digest(bytes));
    let image_url = format!("{prefix}{}", STANDARD.encode(bytes));
    Ok(json!({
        "text": format!("Captured a read-only screenshot of the current {} frontmost window and attached it as transient image input for the next model step.", target.platform),
        "_structured_result": {
            "success": true,
            "mode": "read_only",
            "capture_scope": "frontmost_window",
            "platform": target.platform,
            "application": target.application,
            "pid": target.pid,
            "window_id": target.window_id,
            "window_title": target.title,
            "window_position": target.position,
            "window_size": target.size,
            "capture_position": target.capture_position,
            "capture_size": target.capture_size,
            "clipped_to_visible_desktop": target.clipped_to_visible_desktop,
            "identity_and_geometry_revalidated_after_capture": true,
            "mime_type": mime_type,
            "size_bytes": bytes.len(),
            "sha256": sha256,
            "persisted": false,
            "sensitive_content_possible": true
        },
        "_model_input": [{
            "type": "input_image",
            "image_url": image_url,
            "detail": "high"
        }]
    }))
}

fn ensure_observation_runtime() -> Result<()> {
    dependency_error_local()
        .map(|error| Err(anyhow!(error)))
        .unwrap_or(Ok(()))
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

fn execute_jxa(script: &str, arguments: &[String]) -> Result<Value> {
    execute_jxa_with_policy(script, arguments, true)
}

fn execute_jxa_action(script: &str, arguments: &[String]) -> Result<Value> {
    execute_jxa_with_policy(script, arguments, false)
}

fn execute_jxa_with_policy(
    script: &str,
    arguments: &[String],
    mark_read_only: bool,
) -> Result<Value> {
    let mut command = Command::new(MACOS_OSASCRIPT_PATH);
    command
        .args(["-l", "JavaScript", "-e", script, "--"])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .context("start macOS Computer Use observer")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Computer Use observer stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Computer Use observer stderr is unavailable"))?;
    let stdout_reader =
        thread::spawn(move || read_limited(stdout, "stdout", COMPUTER_USE_OUTPUT_MAX_BYTES));
    let stderr_reader =
        thread::spawn(move || read_limited(stderr, "stderr", COMPUTER_USE_STDERR_MAX_BYTES));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().context("poll Computer Use observer")? {
            break status;
        }
        if started.elapsed() >= COMPUTER_USE_COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(anyhow!(
                "Computer Use observation timed out after {} seconds",
                COMPUTER_USE_COMMAND_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(25));
    };
    let stdout = join_reader(stdout_reader, "stdout")?;
    let stderr = join_reader(stderr_reader, "stderr")?;
    decode_jxa_result_with_policy(status, stdout.as_slice(), stderr.as_slice(), mark_read_only)
}

fn read_limited<R: Read>(mut reader: R, label: &str, limit: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("read Computer Use observer {label}"))?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > limit {
            return Err(anyhow!(
                "Computer Use observer {label} exceeded {limit} bytes"
            ));
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

fn join_reader(handle: thread::JoinHandle<Result<Vec<u8>>>, label: &str) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow!("Computer Use observer {label} reader panicked"))?
}

#[cfg(test)]
fn decode_jxa_result(status: ExitStatus, stdout: &[u8], stderr: &[u8]) -> Result<Value> {
    decode_jxa_result_with_policy(status, stdout, stderr, true)
}

fn decode_jxa_result_with_policy(
    status: ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
    mark_read_only: bool,
) -> Result<Value> {
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if !status.success() {
        return Err(classify_macos_observer_error(stderr.as_str(), status));
    }
    if stdout.is_empty() {
        return Err(anyhow!("Computer Use observer returned no JSON output"));
    }
    let mut value: Value = serde_json::from_str(stdout.as_str())
        .context("decode Computer Use observer JSON output")?;
    let map = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Computer Use observer output must be a JSON object"))?;
    if mark_read_only {
        map.insert("success".to_string(), Value::Bool(true));
        map.insert("mode".to_string(), Value::String("read_only".to_string()));
        map.insert(
            "sensitive_text_policy".to_string(),
            Value::String("editable_values_redacted".to_string()),
        );
    }
    Ok(value)
}

fn classify_macos_observer_error(stderr: &str, status: ExitStatus) -> anyhow::Error {
    let normalized = stderr.to_ascii_lowercase();
    if [
        "not authorized",
        "not allowed assistive access",
        "not permitted",
        "(-1719)",
        "(-1743)",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
    {
        return anyhow!("macOS Accessibility permission is required for Computer Use observation");
    }
    if stderr.is_empty() {
        anyhow!("Computer Use observer failed with status {status}")
    } else {
        anyhow!("Computer Use observer failed: {stderr}")
    }
}

fn classify_macos_screenshot_error(stderr: &str, status: ExitStatus) -> anyhow::Error {
    let normalized = stderr.to_ascii_lowercase();
    if normalized.contains("not authorized")
        || normalized.contains("not permitted")
        || normalized.contains("screen recording")
        || normalized.contains("could not create image from display")
    {
        return anyhow!(
            "macOS Screen Recording permission is required for Computer Use screenshots"
        );
    }
    if stderr.is_empty() {
        anyhow!("Computer Use screenshot failed with status {status}")
    } else {
        anyhow!("Computer Use screenshot failed: {stderr}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn tool_contract_is_read_only_and_bounded() {
        let tools = tool_definitions(false);
        assert_eq!(tools.len(), 7);
        assert_eq!(tools[0]["name"], "computer_list_windows");
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(names.contains(&"computer_capture_main_display"));
        assert!(names.contains(&"computer_capture_frontmost_window"));
        assert!(names.contains(&"computer_list_displays"));
        assert!(names.contains(&"computer_capture_display"));
        assert!(names.contains(&"computer_inspect_frontmost_window"));
        assert!(names.contains(&"computer_capture_window_layout"));
        assert!(tools.iter().all(|tool| tool["description"]
            .as_str()
            .is_some_and(|description| description.contains("Read-only"))));
        assert!(tools
            .iter()
            .all(|tool| tool.pointer("/inputSchema/additionalProperties")
                == Some(&Value::Bool(false))));
    }

    #[test]
    fn control_tools_are_published_only_for_the_approved_plugin_path() {
        let tools = tool_definitions(true);
        assert_eq!(
            tools.len(),
            if matches!(current_platform_name(), "macos" | "windows") {
                16
            } else {
                15
            }
        );
        let find = |name: &str| {
            tools
                .iter()
                .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
                .expect("published Computer Use tool")
        };
        assert!(find("computer_click")["description"]
            .as_str()
            .is_some_and(|description| description.contains("explicit local user approval")));
        assert!(find("computer_drag")["description"]
            .as_str()
            .is_some_and(|description| description.contains("forces mouse-up")));
        assert!(find("computer_press_key")["description"]
            .as_str()
            .is_some_and(|description| description.contains("one-time random confirmation")));
        assert!(find("computer_scroll").is_object());
        assert!(find("computer_activate_application").is_object());
        assert!(find("computer_set_frontmost_window_bounds")["description"]
            .as_str()
            .is_some_and(|description| description.contains("partial platform failures")));
        assert!(find("computer_restore_window_layout")["description"]
            .as_str()
            .is_some_and(|description| description.contains("snapshot ID")));
        if current_platform_name() == "macos" {
            assert!(
                find("computer_set_frontmost_window_fullscreen")["description"]
                    .as_str()
                    .is_some_and(|description| description.contains("AXFullScreen"))
            );
        }
        if current_platform_name() == "windows" {
            assert!(
                find("computer_set_frontmost_window_maximized")["description"]
                    .as_str()
                    .is_some_and(
                        |description| description.contains("not true application fullscreen")
                    )
            );
        }
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "computer_type_text"));
    }

    #[test]
    fn windows_contract_includes_ui_automation_and_secure_text_entry() {
        let tools = tool_definitions_for_platform(true, "windows");
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(tools.len(), 16);
        assert!(names.contains(&"computer_list_windows"));
        assert!(names.contains(&"computer_capture_window_layout"));
        assert!(names.contains(&"computer_inspect_frontmost_window"));
        assert!(names.contains(&"computer_capture_main_display"));
        assert!(names.contains(&"computer_capture_frontmost_window"));
        assert!(names.contains(&"computer_list_displays"));
        assert!(names.contains(&"computer_capture_display"));
        assert!(names.contains(&"computer_click"));
        assert!(names.contains(&"computer_drag"));
        assert!(names.contains(&"computer_press_key"));
        assert!(names.contains(&"computer_scroll"));
        assert!(names.contains(&"computer_activate_application"));
        assert!(names.contains(&"computer_type_text"));
        assert!(names.contains(&"computer_set_frontmost_window_bounds"));
        assert!(names.contains(&"computer_set_frontmost_window_maximized"));
        assert!(names.contains(&"computer_restore_window_layout"));
        assert!(!names.contains(&"computer_set_frontmost_window_fullscreen"));
        assert!(tools.iter().any(|tool| {
            tool["name"] == "computer_type_text"
                && tool["description"]
                    .as_str()
                    .is_some_and(|description| description.contains("fails closed"))
        }));
        let source = include_str!("computer_use/windows.rs");
        assert!(source.contains("struct MouseButtonReleaseGuard"));
        assert!(source.contains("MouseButtonReleaseGuard::new(up)"));
        assert!(source.contains("MouseButtonReleaseGuard::new(MOUSEEVENTF_LEFTUP)"));
        assert!(source.contains("send_mouse_flags(self.release_flags, 0)"));
        assert!(source.contains("IUIAutomationTextEditPattern"));
        assert!(source.contains("UIA_TextEditPatternId"));
        assert!(source.contains("UIA_DocumentControlTypeId"));
        assert!(source.contains("WindowsTextTargetClass::ContentEditable"));
        assert!(source.contains("pub(super) fn capture_frontmost_window()"));
        assert!(source.contains("same_identity_and_geometry"));
        assert!(source.contains("intersect_rect(window_rect, virtual_desktop_rect()?"));
        assert!(source.contains("SetWindowPos("));
        assert!(source.contains("IsZoomed(hwnd)"));
        assert!(source.contains("restore_window_bounds"));
        assert!(source.contains("restore_window_maximized_state"));
        assert!(source.contains("pub(super) fn capture_window_layout"));
        assert!(source.contains("pub(super) fn restore_window_layout"));
        assert!(source.contains("rollback_layout_windows"));
        assert!(source.contains("WS_EX_TOOLWINDOW"));
        assert!(source.contains("GW_OWNER"));
        assert!(source.contains("WS_CAPTION"));
    }

    #[test]
    fn macos_window_control_contract_uses_native_ax_state_without_shortcuts() {
        let tools = tool_definitions_for_platform(true, "macos");
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(tools.len(), 16);
        assert!(names.contains(&"computer_capture_window_layout"));
        assert!(names.contains(&"computer_restore_window_layout"));
        assert!(names.contains(&"computer_set_frontmost_window_bounds"));
        assert!(names.contains(&"computer_set_frontmost_window_fullscreen"));
        assert!(!names.contains(&"computer_set_frontmost_window_maximized"));
        assert!(FRONTMOST_WINDOW_CONTROL_TARGET_JXA.contains("AXPosition"));
        assert!(FRONTMOST_WINDOW_CONTROL_TARGET_JXA.contains("AXSize"));
        assert!(FRONTMOST_WINDOW_CONTROL_TARGET_JXA.contains("AXFullScreen"));
        assert!(SET_FRONTMOST_WINDOW_BOUNDS_JXA.contains("matchesApproved(before, approved)"));
        assert!(SET_FRONTMOST_WINDOW_BOUNDS_JXA.contains("recoveryResult(approved)"));
        assert!(SET_FRONTMOST_WINDOW_FULLSCREEN_JXA
            .contains("fullscreen_attribute.value.set(requested)"));
        assert!(RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA
            .contains("fullscreen_attribute.value.set(approved.fullscreen)"));
        assert!(CAPTURE_WINDOW_LAYOUT_JXA.contains("AXStandardWindow"));
        assert!(PREFLIGHT_WINDOW_LAYOUT_JXA.contains("process_identity"));
        assert!(RESTORE_WINDOW_LAYOUT_JXA.contains("rollback(systemEvents"));
        assert!(RESTORE_WINDOW_LAYOUT_JXA.contains("!recovery.complete"));
        assert!(ROLLBACK_WINDOW_LAYOUT_JXA.contains("snapshot.windows.length - 1"));
        assert!(!SET_FRONTMOST_WINDOW_FULLSCREEN_JXA.contains("keystroke"));
    }

    #[test]
    fn window_bounds_and_approval_guard_are_bounded_and_fail_closed() {
        let request = parse_window_bounds_request(&json!({
            "x": -1200,
            "y": 80,
            "width": 1280,
            "height": 720,
        }))
        .unwrap();
        assert_eq!(request.geometry(), "1280 x 720 @ -1200, 80");
        assert!(parse_window_bounds_request(&json!({
            "x": 0,
            "y": 0,
            "width": 63,
            "height": 720,
        }))
        .is_err());
        assert!(parse_window_bounds_request(&json!({
            "x": 0,
            "y": 0,
            "width": 1280,
            "height": 720,
            "pid": 42,
        }))
        .is_err());
        assert!(parse_window_fullscreen_request(&json!({"fullscreen": true})).unwrap());
        assert!(parse_window_fullscreen_request(&json!({"fullscreen": 1})).is_err());
        assert!(!parse_window_maximized_request(&json!({"maximized": false})).unwrap());

        let guard = ApprovedFrontmostWindowGuard {
            platform: "macos".to_string(),
            application: "Example".to_string(),
            pid: 42,
            window_id: "1001".to_string(),
            position: [10.0, 20.0],
            size: [1280.0, 720.0],
            fullscreen: Some(false),
            maximized: None,
            position_settable: true,
            size_settable: true,
            fullscreen_settable: true,
        };
        let encoded = vec![window_approval_argument(&guard).unwrap()];
        assert!(!encoded.join(" ").contains("title"));
        assert_eq!(approved_window_guard(Some(&encoded)).unwrap(), guard);
        assert!(approved_window_guard(Some(&[])).is_err());
        let display_layout = vec![ApprovedDisplayGuard {
            index: 1,
            display_id: 99,
            is_main: true,
            origin_x: 0.0,
            origin_y: 0.0,
            width: 1920.0,
            height: 1080.0,
            pixels_wide: 3840,
            pixels_high: 2160,
            rotation_degrees: 0.0,
        }];
        validate_requested_window_bounds_against_layout(&request, &display_layout).unwrap();
        let encoded_layout =
            vec![window_display_layout_approval_argument(&display_layout).unwrap()];
        assert_eq!(
            approved_window_display_layout(Some(&encoded_layout)).unwrap(),
            display_layout
        );
        assert!(approved_window_display_layout(Some(&[])).is_err());
        let mut invalid = guard.clone();
        invalid.fullscreen = None;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn window_layout_snapshot_is_opaque_bounded_one_shot_and_redacted() {
        let display_layout = vec![ApprovedDisplayGuard {
            index: 1,
            display_id: 99,
            is_main: true,
            origin_x: 0.0,
            origin_y: 0.0,
            width: 1920.0,
            height: 1080.0,
            pixels_wide: 3840,
            pixels_high: 2160,
            rotation_degrees: 0.0,
        }];
        let window = ApprovedWindowLayoutGuard {
            platform: current_platform_name().to_string(),
            application: "Example".to_string(),
            process_identity: "bundle:com.example.app".to_string(),
            pid: 42,
            window_id: "1001".to_string(),
            position: [100.0, 120.0],
            size: [1280.0, 720.0],
        };
        let mut snapshot = WindowLayoutSnapshot {
            schema_version: WINDOW_LAYOUT_SCHEMA_VERSION,
            snapshot_id: uuid::Uuid::new_v4().hyphenated().to_string(),
            snapshot_sha256: String::new(),
            platform: current_platform_name().to_string(),
            display_layout,
            windows: vec![window],
            excluded_window_count: 2,
            truncated: false,
        };
        snapshot.snapshot_sha256 = window_layout_sha256(&snapshot).unwrap();
        snapshot.validate().unwrap();

        let reference_arguments = json!({
            "snapshot_id": snapshot.snapshot_id,
            "snapshot_sha256": snapshot.snapshot_sha256,
        });
        let reference = parse_window_layout_reference(&reference_arguments).unwrap();
        assert!(parse_window_layout_reference(&json!({
            "snapshot_id": reference.snapshot_id,
            "snapshot_sha256": reference.snapshot_sha256,
            "pid": 42,
        }))
        .is_err());

        let approved_argument = window_layout_approval_argument(&snapshot).unwrap();
        let approved_arguments = vec![approved_argument];
        assert_eq!(
            approved_window_layout_snapshot(Some(approved_arguments.as_slice())).unwrap(),
            snapshot
        );
        assert!(redact_approval_arguments("computer_restore_window_layout"));

        store_window_layout_snapshot(snapshot.clone()).unwrap();
        assert_eq!(stored_window_layout_snapshot(&reference).unwrap(), snapshot);
        consume_approved_window_layout_snapshot(
            &reference_arguments,
            Some(approved_arguments.as_slice()),
        )
        .unwrap();
        assert!(stored_window_layout_snapshot(&reference).is_err());

        let public = finalize_window_layout_capture(
            serde_json::to_value(WindowLayoutCapturePayload {
                platform: snapshot.platform.clone(),
                display_layout: snapshot.display_layout.clone(),
                windows: snapshot.windows.clone(),
                excluded_window_count: 0,
                truncated: false,
            })
            .unwrap(),
        )
        .unwrap();
        let serialized = public.to_string();
        assert!(!serialized.contains("com.example.app"));
        assert!(!serialized.contains("1001"));
        assert_eq!(
            public.pointer("/_structured_result/persisted"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            public.pointer("/_structured_result/model_supplied_window_identities_or_coordinates"),
            Some(&Value::Bool(false))
        );

        let now = Instant::now();
        let mut at_capacity = BTreeMap::new();
        for _ in 0..MAX_WINDOW_LAYOUT_SNAPSHOTS {
            let mut stored_snapshot = snapshot.clone();
            stored_snapshot.snapshot_id = uuid::Uuid::new_v4().hyphenated().to_string();
            stored_snapshot.snapshot_sha256 = window_layout_sha256(&stored_snapshot).unwrap();
            at_capacity.insert(
                stored_snapshot.snapshot_id.clone(),
                StoredWindowLayoutSnapshot {
                    captured_at: now,
                    snapshot: stored_snapshot,
                },
            );
        }
        prune_expired_window_layout_snapshots(&mut at_capacity, now);
        assert_eq!(at_capacity.len(), MAX_WINDOW_LAYOUT_SNAPSHOTS);
        evict_window_layout_snapshot_for_insert(&mut at_capacity);
        assert_eq!(at_capacity.len(), MAX_WINDOW_LAYOUT_SNAPSHOTS - 1);
    }

    #[test]
    fn click_contract_supports_single_right_and_approved_left_double_clicks() {
        let tools = tool_definitions(true);
        let click = tools
            .iter()
            .find(|tool| tool["name"] == "computer_click")
            .expect("click tool");
        assert_eq!(
            click.pointer("/inputSchema/properties/click_count/enum"),
            Some(&json!([1, 2]))
        );
        assert!(click["description"]
            .as_str()
            .is_some_and(|description| description.contains("double-click")));
        assert_eq!(parse_click_count(&json!({}), "left").unwrap(), 1);
        assert_eq!(
            parse_click_count(&json!({"click_count":2}), "left").unwrap(),
            2
        );
        assert_eq!(
            parse_click_count(&json!({"click_count":1}), "right").unwrap(),
            1
        );
        assert!(parse_click_count(&json!({"click_count":2}), "right").is_err());
        assert!(parse_click_count(&json!({"click_count":0}), "left").is_err());
        assert!(parse_click_count(&json!({"click_count":3}), "left").is_err());

        let display = DisplayTarget {
            index: 2,
            display_id: 42,
            is_main: false,
            origin_x: -1920.0,
            origin_y: 0.0,
            width: 1920.0,
            height: 1080.0,
            pixels_wide: 3840,
            pixels_high: 2160,
            rotation_degrees: 0.0,
        };
        let action = ClickAction {
            display,
            x: 320.0,
            y: 240.0,
            global_x: -1600.0,
            global_y: 240.0,
            button: "left",
            click_count: 2,
        };
        let approval = click_approval_arguments(&action).unwrap();
        assert!(approval.iter().any(|value| value == "--button=left"));
        assert!(approval.iter().any(|value| value == "--click-count=2"));
        let result = click_result(&action);
        assert_eq!(result["click_count"], 2);
        assert_eq!(result["interruptible_between_clicks"], true);
    }

    #[test]
    fn drag_contract_is_bounded_cancel_aware_and_display_guarded() {
        let tools = tool_definitions(true);
        let drag = tools
            .iter()
            .find(|tool| tool["name"] == "computer_drag")
            .expect("drag tool");
        assert_eq!(
            drag.pointer("/inputSchema/properties/duration_ms/minimum")
                .and_then(Value::as_u64),
            Some(MIN_DRAG_DURATION_MS)
        );
        assert_eq!(
            drag.pointer("/inputSchema/properties/duration_ms/maximum")
                .and_then(Value::as_u64),
            Some(MAX_DRAG_DURATION_MS)
        );
        assert_eq!(drag_step_count(MIN_DRAG_DURATION_MS), 5);
        assert_eq!(drag_step_count(MAX_DRAG_DURATION_MS), MAX_DRAG_STEPS);

        let cancelled = AtomicBool::new(false);
        ensure_action_not_cancelled(Some(&cancelled)).unwrap();
        cancelled.store(true, Ordering::SeqCst);
        assert!(ensure_action_not_cancelled(Some(&cancelled)).is_err());

        let display = DisplayTarget {
            index: 2,
            display_id: 42,
            is_main: false,
            origin_x: -1920.0,
            origin_y: 0.0,
            width: 1920.0,
            height: 1080.0,
            pixels_wide: 3840,
            pixels_high: 2160,
            rotation_degrees: 0.0,
        };
        let approved = vec![display_approval_argument(&display).unwrap()];
        validate_approved_display(&display, Some(approved.as_slice())).unwrap();
        let mut drifted = display.clone();
        drifted.origin_x = 0.0;
        assert!(validate_approved_display(&drifted, Some(approved.as_slice())).is_err());
        assert!(validate_approved_display(&display, Some(&[])).is_err());
    }

    #[test]
    fn key_action_contract_is_allowlisted_and_stable_for_approval() {
        let (command, args, audit) = approval_command(
            "computer_press_key",
            &json!({"key":"enter", "modifiers":["shift", "command"]}),
        )
        .unwrap();
        assert_eq!(command, "computer_press_key");
        assert_eq!(args, ["--key=enter", "--modifiers=command+shift"]);
        assert_eq!(audit.kind, "computer_use");
        assert_eq!(audit.operation, "computer_press_key");
        assert!(audit
            .details
            .iter()
            .any(|detail| detail.key == "modifiers" && detail.value == "command+shift"));
        assert!(audit.details.iter().any(|detail| {
            detail.key == "confirmation_risk" && detail.value == "submit_or_activate"
        }));
        let (_, _, escape_audit) = approval_command(
            "computer_press_key",
            &json!({"key":"escape", "modifiers":[]}),
        )
        .unwrap();
        assert!(!escape_audit
            .details
            .iter()
            .any(|detail| detail.key == "confirmation_risk"));
        assert!(
            approval_command("computer_press_key", &json!({"key":"a", "modifiers":[]})).is_err()
        );
        assert!(approval_command(
            "computer_press_key",
            &json!({"key":"enter", "modifiers":["shift", "shift"]})
        )
        .is_err());
    }

    #[test]
    fn typed_text_is_visible_for_approval_but_not_for_history_or_results() {
        let secret = "review this exact text";
        let (command, args, audit) =
            approval_command("computer_type_text", &json!({"text": secret})).unwrap();
        assert_eq!(command, "computer_type_text");
        assert!(args.join(" ").contains(secret));
        assert!(redact_approval_arguments("computer_type_text"));
        let serialized_audit = serde_json::to_string(&audit).unwrap();
        assert!(!serialized_audit.contains(secret));
        assert_eq!(
            audit.privacy.as_deref(),
            Some("text_redacted_from_persistent_history")
        );
        assert!(audit.details.iter().any(|detail| {
            detail.key == "text_sha256"
                && detail.value == hex::encode(Sha256::digest(secret.as_bytes()))
        }));
        assert!(audit.details.iter().any(|detail| {
            detail.key == "confirmation_risk" && detail.value == "sensitive_text_entry"
        }));

        let arguments = json!({"text": secret});
        let action = parse_typed_text(&arguments).unwrap();
        let result = typed_text_result(&action);
        assert!(!result.to_string().contains(secret));
        assert_eq!(result["character_count"], secret.chars().count());
        assert_eq!(result["text_persisted"], false);
        assert_eq!(
            result["sha256"],
            hex::encode(Sha256::digest(secret.as_bytes()))
        );
    }

    #[test]
    fn typed_text_rejects_controls_invisible_formatting_and_oversize_input() {
        assert!(parse_typed_text(&json!({"text": "line one\nline two"})).is_err());
        assert!(parse_typed_text(&json!({"text": "safe\u{202e}spoof"})).is_err());
        assert!(parse_typed_text(&json!({"text": "x".repeat(257)})).is_err());
    }

    #[test]
    fn scroll_contract_is_bounded_non_zero_and_stable_for_approval() {
        let (command, args, audit) =
            approval_command("computer_scroll", &json!({"delta_y": -240, "delta_x": 20})).unwrap();
        assert_eq!(command, "computer_scroll");
        assert_eq!(args, ["--delta-y=-240", "--delta-x=20"]);
        assert!(audit
            .details
            .iter()
            .any(|detail| detail.key == "delta_y" && detail.value == "-240"));
        assert!(parse_scroll(&json!({})).is_err());
        assert!(parse_scroll(&json!({"delta_y": 1201})).is_err());
        assert!(parse_scroll(&json!({"delta_y": 1.5})).is_err());
    }

    #[test]
    fn application_activation_accepts_only_a_positive_pid_and_sanitizes_labels() {
        assert_eq!(parse_application_pid(&json!({"pid": 42})).unwrap(), 42);
        assert!(parse_application_pid(&json!({"pid": 0})).is_err());
        assert!(parse_application_pid(&json!({"pid": "42"})).is_err());
        assert_eq!(safe_approval_label("Safe\u{202e}Name"), "Safe�Name");
        assert_eq!(
            approved_application_name(Some(&["--application-json=\"Safari\"".to_string()]))
                .unwrap(),
            "Safari"
        );
        assert!(approved_application_name(Some(&[])).is_err());
        assert!(ACTIVATE_APPLICATION_JXA.contains("argv[1]"));
        assert!(ACTIVATE_APPLICATION_JXA.contains("approved application identity changed"));
        assert!(
            ACTIVATE_APPLICATION_JXA.contains("frontmost application changed before activation")
        );
        assert!(ACTIVATE_APPLICATION_JXA.contains("frontmost.set(true)"));
        assert!(FRONTMOST_APPLICATION_JXA.contains("candidate.frontmost()"));
        assert!(RESTORE_APPLICATION_JXA.contains("target.frontmost()"));
        assert!(RESTORE_APPLICATION_JXA.contains("previous.frontmost.set(true)"));
        assert!(RESTORE_APPLICATION_JXA.contains("foreground_changed_after_activation"));
        let source = include_str!("computer_use/windows.rs");
        assert!(source.contains("ApplicationActivationRollbackGuard"));
        assert!(source.contains("foreground application changed before activation"));
        assert!(source.contains("GetForegroundWindow() != target_hwnd"));
        assert!(source.contains("SetForegroundWindow(previous_hwnd)"));
        assert!(source.contains("target_was_minimized"));
    }

    #[test]
    fn application_activation_recovery_metadata_is_narrow_and_never_replay_safe() {
        let action = with_application_activation_recovery(
            json!({"success": true, "action": "activate_application"}),
            json!({
                "scope": "frontmost_application_activation_only",
                "rollback_on_in_flight_cancel": true,
                "attempted": true,
                "restored": true,
                "reason": "cancelled_activation_restored",
                "application_content_rollback": false,
                "window_geometry_rollback": false,
            }),
        );
        let result = build_post_action_result(
            "computer_activate_application",
            action,
            &PostActionObservationTarget::MainDisplay,
            Err("cancelled_after_action"),
        );
        let structured = &result["_structured_result"];
        assert_eq!(
            structured["application_state_recovery"]["scope"],
            "frontmost_application_activation_only"
        );
        assert_eq!(structured["application_state_recovery"]["restored"], true);
        assert_eq!(
            structured["application_state_recovery"]["application_content_rollback"],
            false
        );
        assert_eq!(structured["recovery"]["automatic_replay_safe"], false);
    }

    #[test]
    fn bounded_integer_rejects_invalid_ranges_and_types() {
        assert_eq!(
            bounded_integer(&json!({}), "limit", 40, 1, 100).unwrap(),
            40
        );
        assert!(bounded_integer(&json!({"limit": 0}), "limit", 40, 1, 100).is_err());
        assert!(bounded_integer(&json!({"limit": "40"}), "limit", 40, 1, 100).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decoded_observation_is_marked_read_only() {
        let status = ExitStatus::from_raw(0);
        let value = decode_jxa_result(status, br#"{"platform":"macos"}"#, b"").unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["mode"], "read_only");
        assert_eq!(value["sensitive_text_policy"], "editable_values_redacted");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn permission_failures_are_reported_without_raw_automation_noise() {
        let status = ExitStatus::from_raw(1 << 8);
        let error = classify_macos_observer_error(
            "System Events got an error: osascript is not allowed assistive access. (-1719)",
            status,
        );
        assert_eq!(
            error.to_string(),
            "macOS Accessibility permission is required for Computer Use observation"
        );
    }

    #[test]
    fn editable_values_are_redacted_by_the_embedded_inspection_script() {
        assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("role === \"AXTextField\""));
        assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("AXIsEditable"));
        assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("AXEditableAncestor"));
        assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("if (editable)"));
        assert!(INSPECT_FRONTMOST_WINDOW_JXA.contains("node.value_redacted = true"));
        assert!(INSPECT_FRONTMOST_WINDOW_JXA
            .find("if (editable)")
            .is_some_and(|editable| INSPECT_FRONTMOST_WINDOW_JXA
                .find("node.value = text")
                .is_some_and(|value| editable < value)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_text_target_classifier_requires_explicit_writability() {
        assert_eq!(
            classify_macos_text_target(true, false, false, true, false).unwrap(),
            MacTextTargetClass::NativeTextControl
        );
        assert_eq!(
            classify_macos_text_target(false, true, true, false, true).unwrap(),
            MacTextTargetClass::ContentEditable
        );
        assert!(classify_macos_text_target(true, false, false, false, false).is_err());
        assert!(classify_macos_text_target(false, true, false, false, true).is_err());
        assert!(classify_macos_text_target(false, true, true, false, false).is_err());
        assert!(classify_macos_text_target(false, false, true, true, true).is_err());
    }

    #[test]
    fn screenshot_payload_separates_transient_image_from_persistable_metadata() {
        let display = DisplayTarget {
            index: 2,
            display_id: 99,
            is_main: false,
            origin_x: 1440.0,
            origin_y: 0.0,
            width: 1920.0,
            height: 1080.0,
            pixels_wide: 3840,
            pixels_high: 2160,
            rotation_degrees: 0.0,
        };
        let result = screenshot_result(&[0xff, 0xd8, 0xff, 0x00, 0x01], &display).unwrap();
        assert!(result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/jpeg;base64,")));
        let structured = result
            .get("_structured_result")
            .expect("structured metadata");
        assert_eq!(structured["persisted"], false);
        assert_eq!(structured["display_index"], 2);
        assert_eq!(structured["capture_scope"], "selected_display");
        assert!(!structured.to_string().contains("base64"));
    }

    #[test]
    fn frontmost_window_screenshot_is_transient_and_geometry_bound() {
        let target = FrontmostWindowCaptureTarget {
            platform: "windows",
            application: "Example.exe".to_string(),
            pid: 42,
            window_id: "0x1234".to_string(),
            title: "Example".to_string(),
            position: [-20.0, 10.0],
            size: [1024.0, 768.0],
            capture_position: [0.0, 10.0],
            capture_size: [1004.0, 768.0],
            clipped_to_visible_desktop: true,
        };
        let result =
            frontmost_window_screenshot_result(b"\x89PNG\r\n\x1a\nwindow-pixels", &target).unwrap();
        assert!(result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        let structured = &result["_structured_result"];
        assert_eq!(structured["capture_scope"], "frontmost_window");
        assert_eq!(structured["window_id"], "0x1234");
        assert_eq!(structured["window_position"], json!([-20.0, 10.0]));
        assert_eq!(structured["capture_position"], json!([0.0, 10.0]));
        assert_eq!(structured["clipped_to_visible_desktop"], true);
        assert_eq!(
            structured["identity_and_geometry_revalidated_after_capture"],
            true
        );
        assert_eq!(structured["persisted"], false);
        assert!(!structured.to_string().contains("base64"));
    }

    #[test]
    fn post_action_observation_attaches_transient_pixels_without_persisting_them() {
        let display = DisplayTarget {
            index: 2,
            display_id: 99,
            is_main: false,
            origin_x: 1440.0,
            origin_y: 0.0,
            width: 1920.0,
            height: 1080.0,
            pixels_wide: 3840,
            pixels_high: 2160,
            rotation_degrees: 0.0,
        };
        let screenshot = screenshot_result(&[0xff, 0xd8, 0xff, 0x00, 0x01], &display).unwrap();
        let target =
            PostActionObservationTarget::ApprovedDisplay(ApprovedDisplayGuard::from(&display));
        let result = build_post_action_result(
            "computer_click",
            json!({"success": true, "action": "click"}),
            &target,
            Ok(screenshot),
        );
        assert!(result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("data:image/jpeg;base64,")));
        let structured = result
            .get("_structured_result")
            .expect("post-action structured result");
        assert_eq!(structured["success"], true);
        assert_eq!(structured["post_action_observation"]["captured"], true);
        assert_eq!(structured["post_action_observation"]["persisted"], false);
        assert_eq!(structured["recovery"]["automatic_replay_safe"], false);
        assert!(!structured.to_string().contains("base64"));
    }

    #[test]
    fn window_post_action_observation_binds_exact_window_and_requested_state() {
        let approved = ApprovedFrontmostWindowGuard {
            platform: "windows".to_string(),
            application: "Example.exe".to_string(),
            pid: 42,
            window_id: "0x1234".to_string(),
            position: [10.0, 20.0],
            size: [800.0, 600.0],
            fullscreen: None,
            maximized: Some(false),
            position_settable: true,
            size_settable: true,
            fullscreen_settable: false,
        };
        let guard = WindowControlRollbackGuard::Bounds {
            request: WindowBoundsRequest {
                x: 120,
                y: 80,
                width: 1000,
                height: 700,
            },
            approved,
        };
        let current = ApprovedFrontmostWindowGuard {
            position: [120.0, 80.0],
            size: [1000.0, 700.0],
            ..match &guard {
                WindowControlRollbackGuard::Bounds { approved, .. } => approved.clone(),
                _ => unreachable!(),
            }
        };
        assert!(guard.matches_applied_state(&current));

        let screenshot_target = FrontmostWindowCaptureTarget {
            platform: "windows",
            application: "Example.exe".to_string(),
            pid: 42,
            window_id: "0x1234".to_string(),
            title: "A dynamic title is not identity".to_string(),
            position: [120.0, 80.0],
            size: [1000.0, 700.0],
            capture_position: [120.0, 80.0],
            capture_size: [1000.0, 700.0],
            clipped_to_visible_desktop: false,
        };
        let screenshot = frontmost_window_screenshot_result(
            b"\x89PNG\r\n\x1a\nwindow-observation",
            &screenshot_target,
        )
        .unwrap();
        let target = guard.observation_target();
        let result = build_post_action_result(
            "computer_set_frontmost_window_bounds",
            json!({"success": true, "target_geometry_applied": true}),
            &target,
            Ok(screenshot),
        );
        assert_eq!(
            result["_structured_result"]["post_action_observation"]["target"]["scope"],
            "frontmost_window"
        );
        assert_eq!(
            result["_structured_result"]["post_action_observation"]["captured"],
            true
        );

        let restored = match &guard {
            WindowControlRollbackGuard::Bounds { approved, .. } => approved.clone(),
            _ => unreachable!(),
        };
        assert!(guard.matches_target_identity(&restored));
        assert!(!guard.matches_applied_state(&restored));

        let changed = ApprovedFrontmostWindowGuard {
            window_id: "0x9999".to_string(),
            ..current
        };
        assert!(!guard.matches_applied_state(&changed));
    }

    #[test]
    fn post_action_observation_failure_never_marks_an_executed_action_for_replay() {
        let target = PostActionObservationTarget::MainDisplay;
        let result = build_post_action_result(
            "computer_type_text",
            json!({
                "success": true,
                "action": "type_text",
                "character_count": 8,
                "sha256": "redacted-hash"
            }),
            &target,
            Err("capture_timeout"),
        );
        assert!(result.get("_model_input").is_none());
        let structured = result
            .get("_structured_result")
            .expect("post-action failure metadata");
        assert_eq!(structured["success"], true);
        assert_eq!(structured["post_action_observation"]["captured"], false);
        assert_eq!(
            structured["post_action_observation"]["reason"],
            "capture_timeout"
        );
        assert_eq!(structured["recovery"]["action_already_executed"], true);
        assert_eq!(structured["recovery"]["automatic_replay_safe"], false);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn screenshot_permission_failures_hide_raw_capture_noise() {
        let status = ExitStatus::from_raw(1 << 8);
        let error = classify_macos_screenshot_error(
            "screencapture: could not create image from display 1",
            status,
        );
        assert_eq!(
            error.to_string(),
            "macOS Screen Recording permission is required for Computer Use screenshots"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn embedded_jxa_observers_compile_without_requesting_permissions() {
        let temp = tempfile::tempdir().expect("temporary script directory");
        for (index, script) in [
            LIST_WINDOWS_JXA,
            CAPTURE_WINDOW_LAYOUT_JXA,
            PREFLIGHT_WINDOW_LAYOUT_JXA,
            RESTORE_WINDOW_LAYOUT_JXA,
            ROLLBACK_WINDOW_LAYOUT_JXA,
            INSPECT_FRONTMOST_WINDOW_JXA,
            FRONTMOST_WINDOW_CAPTURE_TARGET_JXA,
            FRONTMOST_WINDOW_CONTROL_TARGET_JXA,
            SET_FRONTMOST_WINDOW_BOUNDS_JXA,
            RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA,
            SET_FRONTMOST_WINDOW_FULLSCREEN_JXA,
            RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA,
            LOOKUP_APPLICATION_JXA,
            ACTIVATE_APPLICATION_JXA,
            FRONTMOST_APPLICATION_JXA,
            RESTORE_APPLICATION_JXA,
        ]
        .into_iter()
        .enumerate()
        {
            let output_path = temp.path().join(format!("observer-{index}.scpt"));
            let output = Command::new("/usr/bin/osacompile")
                .args(["-l", "JavaScript", "-e", script, "-o"])
                .arg(output_path.as_os_str())
                .output()
                .expect("compile embedded JXA observer");
            assert!(
                output.status.success(),
                "JXA compilation failed: {}",
                String::from_utf8_lossy(output.stderr.as_slice())
            );
        }
    }
}
