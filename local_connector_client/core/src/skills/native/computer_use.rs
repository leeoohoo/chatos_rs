// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(target_os = "macos")]
mod helper;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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
const POST_ACTION_SETTLE_DELAY: Duration = Duration::from_millis(160);

const CONTROL_OPERATIONS: [&str; 6] = [
    "computer_click",
    "computer_drag",
    "computer_press_key",
    "computer_type_text",
    "computer_scroll",
    "computer_activate_application",
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
    title: text(safe(function() { return window.name(); }, ""), 500),
    position: position,
    size: size
  });
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
        ]);
    }
    filter_tools_for_platform(&mut tools, current_platform_name());
    tools
}

fn filter_tools_for_platform(_tools: &mut Vec<Value>, _platform: &str) {}

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
    operation == "computer_type_text"
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
    helper::execute(operation, arguments)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    execute_local(operation, arguments)
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
}

impl PostActionObservationTarget {
    fn requested_index(&self) -> Option<u32> {
        match self {
            Self::MainDisplay => None,
            Self::ApprovedDisplay(display) => Some(display.index),
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
                    post_action_observation_failure(target, "display_identity_changed"),
                );
                return json!({
                    "text": "The approved Computer Use action completed, but its post-action display identity changed. Do not replay the action automatically; observe the desktop again before deciding what to do next.",
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
                "text": "The approved Computer Use action completed. A transient post-action screenshot is attached for recovery and the next model step; its pixels are not persisted.",
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
                "text": "The approved Computer Use action completed, but the automatic post-action screenshot was unavailable. Do not replay the action automatically; observe the desktop again before deciding whether another action is needed.",
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
    decode_jxa_result(status, stdout.as_slice(), stderr.as_slice())
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

fn decode_jxa_result(status: ExitStatus, stdout: &[u8], stderr: &[u8]) -> Result<Value> {
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
    map.insert("success".to_string(), Value::Bool(true));
    map.insert("mode".to_string(), Value::String("read_only".to_string()));
    map.insert(
        "sensitive_text_policy".to_string(),
        Value::String("editable_values_redacted".to_string()),
    );
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
        assert_eq!(tools.len(), 6);
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
        assert_eq!(tools.len(), 12);
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
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "computer_type_text"));
    }

    #[test]
    fn windows_contract_includes_ui_automation_and_secure_text_entry() {
        let mut tools = tool_definitions(true);
        filter_tools_for_platform(&mut tools, "windows");
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(tools.len(), 12);
        assert!(names.contains(&"computer_list_windows"));
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
            INSPECT_FRONTMOST_WINDOW_JXA,
            FRONTMOST_WINDOW_CAPTURE_TARGET_JXA,
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
