// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::io::Write;
use std::mem::size_of;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::Duration;

use ::windows::Win32::Foundation::{HWND as AutomationHwnd, RPC_E_CHANGED_MODE};
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use ::windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationTextEditPattern, IUIAutomationValuePattern, TreeScope_Children,
    UIA_CustomControlTypeId, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
    UIA_PaneControlTypeId, UIA_TextEditPatternId, UIA_ValuePatternId,
};
use anyhow::{anyhow, Context, Result};
use crc32fast::Hasher as Crc32;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetDIBits, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, HMONITOR,
    MONITORINFOEXW, SRCCOPY,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, VK_BACK, VK_DOWN, VK_END, VK_ESCAPE,
    VK_HOME, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_NEXT, VK_PRIOR, VK_RETURN,
    VK_RIGHT, VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetSystemMetrics, GetWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, IsZoomed, SetForegroundWindow, SetWindowPos, ShowWindow, GWL_EXSTYLE,
    GWL_STYLE, GW_OWNER, MONITORINFOF_PRIMARY, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
    SW_MAXIMIZE, SW_RESTORE, WS_CAPTION, WS_EX_TOOLWINDOW,
};

use super::{
    click_result, drag_step_count, ensure_action_not_cancelled, frontmost_window_screenshot_result,
    is_unsafe_typed_character, screenshot_result, typed_text_result, ApprovedFrontmostWindowGuard,
    ApprovedWindowLayoutGuard, ClickAction, DisplayTarget, DragAction,
    FrontmostWindowCaptureTarget, KeyAction, ScrollAction, TypedTextAction, WindowBoundsRequest,
    WindowLayoutCapturePayload, WindowLayoutSnapshot,
};

const MAX_WINDOWS_PER_PROCESS: usize = 20;
const MAX_PROCESS_IMAGE_UNITS: usize = 32_768;
const MAX_CAPTURE_RAW_BYTES: usize = 128 * 1024 * 1024;
const MAX_UI_TEXT_CHARS: usize = 500;
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(60);

#[derive(Debug)]
struct WindowRecord {
    pid: u32,
    title: String,
    position: [i32; 2],
    size: [i32; 2],
    frontmost: bool,
}

struct WindowEnumeration {
    maximum_windows: usize,
    windows: Vec<WindowRecord>,
}

struct WindowLayoutEnumeration {
    maximum_windows: usize,
    windows: Vec<ApprovedWindowLayoutGuard>,
    excluded_window_count: usize,
    truncated: bool,
}

#[derive(Clone)]
struct LiveLayoutWindow {
    hwnd: HWND,
    position: [i32; 2],
    size: [i32; 2],
}

#[derive(Clone)]
struct ForegroundWindowCaptureIdentity {
    hwnd: usize,
    pid: u32,
    application: String,
    title: String,
    window_rect: RECT,
    capture_rect: RECT,
}

impl ForegroundWindowCaptureIdentity {
    fn same_identity_and_geometry(&self, other: &Self) -> bool {
        self.hwnd == other.hwnd
            && self.pid == other.pid
            && self.application == other.application
            && rect_equals(&self.window_rect, &other.window_rect)
            && rect_equals(&self.capture_rect, &other.capture_rect)
    }

    fn capture_target(&self) -> FrontmostWindowCaptureTarget {
        let window_width = self.window_rect.right.saturating_sub(self.window_rect.left);
        let window_height = self.window_rect.bottom.saturating_sub(self.window_rect.top);
        let capture_width = self
            .capture_rect
            .right
            .saturating_sub(self.capture_rect.left);
        let capture_height = self
            .capture_rect
            .bottom
            .saturating_sub(self.capture_rect.top);
        FrontmostWindowCaptureTarget {
            platform: "windows",
            application: self.application.clone(),
            pid: self.pid,
            window_id: format!("0x{:x}", self.hwnd),
            title: self.title.clone(),
            position: [
                f64::from(self.window_rect.left),
                f64::from(self.window_rect.top),
            ],
            size: [f64::from(window_width), f64::from(window_height)],
            capture_position: [
                f64::from(self.capture_rect.left),
                f64::from(self.capture_rect.top),
            ],
            capture_size: [f64::from(capture_width), f64::from(capture_height)],
            clipped_to_visible_desktop: !rect_equals(&self.window_rect, &self.capture_rect),
        }
    }
}

pub(super) fn list_windows(limit: u64) -> Result<Value> {
    let mut enumeration = WindowEnumeration {
        maximum_windows: (limit as usize).saturating_mul(MAX_WINDOWS_PER_PROCESS),
        windows: Vec::new(),
    };
    // SAFETY: the callback receives a valid pointer to enumeration for the synchronous lifetime of
    // EnumWindows. It copies only bounded title/geometry/process identifiers into owned Rust data.
    let completed = unsafe {
        EnumWindows(
            Some(collect_window),
            (&mut enumeration as *mut WindowEnumeration) as LPARAM,
        )
    };
    if completed == 0 {
        return Err(anyhow!("Windows top-level window enumeration failed"));
    }

    let mut processes = BTreeMap::<u32, (String, bool, Vec<Value>)>::new();
    for window in enumeration.windows {
        let Ok(application) = process_name(window.pid) else {
            continue;
        };
        let entry = processes
            .entry(window.pid)
            .or_insert_with(|| (application, false, Vec::new()));
        entry.1 |= window.frontmost;
        if entry.2.len() < MAX_WINDOWS_PER_PROCESS {
            entry.2.push(json!({
                "title": window.title,
                "position": window.position,
                "size": window.size,
            }));
        }
    }
    let mut rows = processes
        .into_iter()
        .map(|(pid, (application, frontmost, windows))| {
            json!({
                "name": application,
                "pid": pid,
                "frontmost": frontmost,
                "windows": windows,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right["frontmost"]
            .as_bool()
            .cmp(&left["frontmost"].as_bool())
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });
    rows.truncate(limit as usize);
    Ok(json!({
        "success": true,
        "mode": "read_only",
        "platform": "windows",
        "process_count": rows.len(),
        "processes": rows,
        "sensitive_text_policy": "window_titles_only",
    }))
}

pub(super) fn capture_window_layout(maximum_windows: usize) -> Result<WindowLayoutCapturePayload> {
    if maximum_windows == 0 || maximum_windows > 8 {
        return Err(anyhow!("Windows window layout limit is invalid"));
    }
    let mut enumeration = WindowLayoutEnumeration {
        maximum_windows,
        windows: Vec::new(),
        excluded_window_count: 0,
        truncated: false,
    };
    // SAFETY: the callback borrows enumeration only for the synchronous EnumWindows call and
    // copies bounded native identities and geometry into owned Rust values.
    let completed = unsafe {
        EnumWindows(
            Some(collect_layout_window),
            (&mut enumeration as *mut WindowLayoutEnumeration) as LPARAM,
        )
    };
    if completed == 0 {
        return Err(anyhow!("Windows window layout enumeration failed"));
    }
    if enumeration.windows.is_empty() {
        return Err(anyhow!(
            "No ordinary restorable Windows windows are available"
        ));
    }
    Ok(WindowLayoutCapturePayload {
        platform: "windows".to_string(),
        display_layout: Vec::new(),
        windows: enumeration.windows,
        excluded_window_count: enumeration.excluded_window_count,
        truncated: enumeration.truncated,
    })
}

pub(super) fn preflight_window_layout(snapshot: &WindowLayoutSnapshot) -> Result<Value> {
    snapshot.validate()?;
    for window in &snapshot.windows {
        live_layout_window(window)?;
    }
    Ok(json!({
        "validated": true,
        "window_count": snapshot.windows.len(),
    }))
}

pub(super) fn restore_window_layout(
    snapshot: &WindowLayoutSnapshot,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    snapshot.validate()?;
    ensure_action_not_cancelled(action_cancelled)?;
    let before = snapshot
        .windows
        .iter()
        .map(live_layout_window)
        .collect::<Result<Vec<_>>>()?;
    let mut applied = Vec::<usize>::new();
    for (index, target) in snapshot.windows.iter().enumerate() {
        if action_cancelled
            .is_some_and(|cancelled| cancelled.load(std::sync::atomic::Ordering::SeqCst))
        {
            if applied.is_empty() {
                return Err(anyhow!("Computer Use action was cancelled"));
            }
            return Ok(window_layout_failure_result(
                snapshot,
                before.as_slice(),
                applied.as_slice(),
                "cancelled_after_action",
                None,
            ));
        }
        let current = live_layout_window(target)?;
        if current.hwnd != before[index].hwnd
            || current.position != before[index].position
            || current.size != before[index].size
        {
            return Ok(window_layout_failure_result(
                snapshot,
                before.as_slice(),
                applied.as_slice(),
                "window_drift_during_restore",
                None,
            ));
        }
        let Some((x, y, width, height)) = layout_guard_geometry_i32(target) else {
            return Err(anyhow!(
                "approved Windows window layout geometry is invalid"
            ));
        };
        // SAFETY: current is the exact live top-level HWND revalidated from the approved snapshot.
        // The target geometry was captured locally and validated against the unchanged display
        // layout. These flags do not activate or reorder the window.
        let changed = unsafe {
            SetWindowPos(
                current.hwnd,
                null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            )
        } != 0;
        let readback = live_layout_window(target).ok();
        let exact = changed
            && readback.as_ref().is_some_and(|window| {
                window.hwnd == current.hwnd
                    && window.position == [x, y]
                    && window.size == [width, height]
            });
        if !exact {
            return Ok(window_layout_failure_result(
                snapshot,
                before.as_slice(),
                applied.as_slice(),
                if changed {
                    "target_geometry_readback_mismatch"
                } else {
                    "platform_apply_failed"
                },
                Some(index),
            ));
        }
        applied.push(index);
        if action_cancelled
            .is_some_and(|cancelled| cancelled.load(std::sync::atomic::Ordering::SeqCst))
        {
            return Ok(window_layout_failure_result(
                snapshot,
                before.as_slice(),
                applied.as_slice(),
                "cancelled_after_action",
                None,
            ));
        }
    }
    thread::sleep(Duration::from_millis(160));
    if action_cancelled.is_some_and(|cancelled| cancelled.load(std::sync::atomic::Ordering::SeqCst))
    {
        return Ok(window_layout_failure_result(
            snapshot,
            before.as_slice(),
            applied.as_slice(),
            "cancelled_after_action",
            None,
        ));
    }
    for target in &snapshot.windows {
        let Some((x, y, width, height)) = layout_guard_geometry_i32(target) else {
            return Err(anyhow!(
                "approved Windows window layout geometry is invalid"
            ));
        };
        let current = live_layout_window(target).ok();
        if !current
            .as_ref()
            .is_some_and(|window| window.position == [x, y] && window.size == [width, height])
        {
            return Ok(window_layout_failure_result(
                snapshot,
                before.as_slice(),
                applied.as_slice(),
                "post_action_window_drift",
                None,
            ));
        }
    }
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "restore_window_layout",
        "platform": "windows",
        "snapshot_id": snapshot.snapshot_id,
        "snapshot_sha256": snapshot.snapshot_sha256,
        "target_window_count": snapshot.windows.len(),
        "restored_window_count": snapshot.windows.len(),
        "identity_geometry_and_display_layout_revalidated": true,
        "automatic_replay_safe": false,
        "application_content_rollback": false,
        "window_layout_recovery": {
            "attempted": false,
            "restored_count": 0,
            "skipped_count": 0,
            "failed_count": 0,
            "complete": false,
        },
    }))
}

struct ComApartment {
    uninitialize: bool,
}

impl ComApartment {
    fn initialize() -> Result<Self> {
        // SAFETY: this initializes COM only for the current execution thread. A successful call is
        // balanced by CoUninitialize; an already initialized apartment is reused without changing
        // or uninitializing the caller's apartment.
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if status.is_ok() {
            return Ok(Self { uninitialize: true });
        }
        if status == RPC_E_CHANGED_MODE {
            return Ok(Self {
                uninitialize: false,
            });
        }
        Err(anyhow!(
            "Windows COM initialization failed with HRESULT 0x{:08x}",
            status.0 as u32
        ))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: this balances the successful CoInitializeEx call made by initialize on the
            // same thread. All UI Automation interface fields are dropped before this guard.
            unsafe { CoUninitialize() };
        }
    }
}

struct UiAutomationSession {
    automation: IUIAutomation,
    _apartment: ComApartment,
}

impl UiAutomationSession {
    fn new() -> Result<Self> {
        let apartment = ComApartment::initialize()?;
        // SAFETY: CUIAutomation is an in-process COM class and the requested interface is the
        // official UI Automation client interface. The owned wrapper releases it on drop.
        let automation = unsafe {
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        }
        .context("create Windows UI Automation client")?;
        Ok(Self {
            automation,
            _apartment: apartment,
        })
    }
}

struct UiTreeContext<'a> {
    condition: &'a IUIAutomationCondition,
    max_depth: u64,
    max_nodes: usize,
    node_count: usize,
    truncated: bool,
}

impl UiTreeContext<'_> {
    fn visit(&mut self, element: &IUIAutomationElement, depth: u64) -> Result<Value> {
        if self.node_count >= self.max_nodes {
            self.truncated = true;
            return Err(anyhow!("Windows UI Automation node limit was reached"));
        }
        self.node_count += 1;
        let node_ref = format!("u{}", self.node_count);

        let control_type = unsafe { element.CurrentControlType() }
            .context("read Windows UI Automation control type")?;
        let role = control_type_name(control_type.0).to_string();
        let subrole = unsafe { element.CurrentLocalizedControlType() }
            .map(|value| bounded_bstr(value, 120))
            .unwrap_or_default();
        let name = unsafe { element.CurrentName() }
            .map(|value| bounded_bstr(value, MAX_UI_TEXT_CHARS))
            .unwrap_or_default();
        let description = unsafe { element.CurrentHelpText() }
            .map(|value| bounded_bstr(value, MAX_UI_TEXT_CHARS))
            .unwrap_or_default();
        let automation_id = unsafe { element.CurrentAutomationId() }
            .map(|value| bounded_bstr(value, 240))
            .unwrap_or_default();
        let class_name = unsafe { element.CurrentClassName() }
            .map(|value| bounded_bstr(value, 240))
            .unwrap_or_default();
        let enabled = unsafe { element.CurrentIsEnabled() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        let focused = unsafe { element.CurrentHasKeyboardFocus() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        let focusable = unsafe { element.CurrentIsKeyboardFocusable() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        let offscreen = unsafe { element.CurrentIsOffscreen() }
            .map(|value| value.as_bool())
            .unwrap_or(true);
        let password = unsafe { element.CurrentIsPassword() }
            .ok()
            .map(|value| value.as_bool());
        let bounds = unsafe { element.CurrentBoundingRectangle() }.ok();
        let value_pattern =
            unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
                .ok();
        let read_only = value_pattern.as_ref().and_then(|pattern| {
            unsafe { pattern.CurrentIsReadOnly() }
                .ok()
                .map(|value| value.as_bool())
        });
        let editable = control_type == UIA_EditControlTypeId && read_only == Some(false);
        let value_redacted = control_type == UIA_EditControlTypeId
            || password != Some(false)
            || value_pattern.is_some();

        let mut children = Vec::new();
        if depth < self.max_depth && self.node_count < self.max_nodes {
            let child_array = unsafe { element.FindAll(TreeScope_Children, self.condition) }
                .context("enumerate Windows UI Automation control children")?;
            let child_count = unsafe { child_array.Length() }
                .context("read Windows UI Automation child count")?
                .max(0) as usize;
            for index in 0..child_count {
                if self.node_count >= self.max_nodes {
                    self.truncated = true;
                    break;
                }
                let child = unsafe { child_array.GetElement(index as i32) }
                    .context("read Windows UI Automation child")?;
                children.push(self.visit(&child, depth + 1)?);
            }
        }

        let mut node = json!({
            "ref": node_ref,
            "role": role,
            "subrole": subrole,
            "control_type_id": control_type.0,
            "name": name,
            "description": description,
            "automation_id": automation_id,
            "class_name": class_name,
            "enabled": enabled,
            "focused": focused,
            "focusable": focusable,
            "offscreen": offscreen,
            "editable": editable,
            "children": children,
        });
        let map = node
            .as_object_mut()
            .ok_or_else(|| anyhow!("Windows UI Automation node serialization failed"))?;
        if value_redacted {
            map.insert("value_redacted".to_string(), Value::Bool(true));
        }
        match password {
            Some(value) => {
                map.insert("password".to_string(), Value::Bool(value));
            }
            None => {
                map.insert("password_state_unknown".to_string(), Value::Bool(true));
            }
        }
        if let Some(read_only) = read_only {
            map.insert("read_only".to_string(), Value::Bool(read_only));
        }
        if let Some(bounds) = bounds {
            let width = bounds.right.saturating_sub(bounds.left);
            let height = bounds.bottom.saturating_sub(bounds.top);
            if width > 0 && height > 0 {
                map.insert("position".to_string(), json!([bounds.left, bounds.top]));
                map.insert("size".to_string(), json!([width, height]));
            }
        }
        Ok(node)
    }
}

pub(super) fn inspect_frontmost_window(max_depth: u64, max_nodes: u64) -> Result<Value> {
    let (hwnd, pid) = foreground_window()?;
    let session = UiAutomationSession::new()?;
    // SAFETY: hwnd is the current non-null foreground top-level window. UI Automation returns an
    // owned element wrapper and does not transfer ownership of the HWND.
    let root = unsafe { session.automation.ElementFromHandle(AutomationHwnd(hwnd)) }
        .context("inspect the Windows foreground window with UI Automation")?;
    let root_pid = unsafe { root.CurrentProcessId() }
        .context("read Windows UI Automation root process identity")?;
    if root_pid <= 0 || root_pid as u32 != pid {
        return Err(anyhow!(
            "Windows foreground window identity changed before UI Automation inspection"
        ));
    }
    let condition = unsafe { session.automation.ControlViewCondition() }
        .context("create Windows UI Automation control-view condition")?;
    let mut context = UiTreeContext {
        condition: &condition,
        max_depth,
        max_nodes: max_nodes as usize,
        node_count: 0,
        truncated: false,
    };
    let tree = context.visit(&root, 0)?;
    Ok(json!({
        "success": true,
        "mode": "read_only",
        "platform": "windows",
        "application": process_name(pid)?,
        "pid": pid,
        "window_title": unsafe { window_title(hwnd) },
        "node_count": context.node_count,
        "truncated": context.truncated,
        "text_entry_values_redacted": true,
        "sensitive_text_policy": "editable_values_redacted",
        "tree": tree,
    }))
}

fn foreground_window() -> Result<(HWND, u32)> {
    // SAFETY: GetForegroundWindow returns a borrowed HWND and GetWindowThreadProcessId writes one
    // process identifier for that current handle.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Err(anyhow!("Windows has no current foreground window"));
    }
    let mut pid = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return Err(anyhow!(
            "Windows foreground process identity is unavailable"
        ));
    }
    Ok((hwnd, pid))
}

fn foreground_window_capture_identity() -> Result<ForegroundWindowCaptureIdentity> {
    let (hwnd, pid) = foreground_window()?;
    // SAFETY: hwnd is the live foreground top-level window. These calls only read its current
    // visibility, minimized state, bounds, and title.
    if unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { IsIconic(hwnd) } != 0 {
        return Err(anyhow!(
            "Windows foreground window is not visibly capturable"
        ));
    }
    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window_rect) } == 0
        || window_rect.right <= window_rect.left
        || window_rect.bottom <= window_rect.top
    {
        return Err(anyhow!("Windows foreground window geometry is invalid"));
    }
    let capture_rect = intersect_rect(window_rect, virtual_desktop_rect()?)
        .ok_or_else(|| anyhow!("Windows foreground window has no visible desktop pixels"))?;
    let application = process_name(pid)?;
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(anyhow!(
            "Windows foreground window changed during capture-target discovery"
        ));
    }
    Ok(ForegroundWindowCaptureIdentity {
        hwnd: hwnd as usize,
        pid,
        application,
        title: unsafe { window_title(hwnd) },
        window_rect,
        capture_rect,
    })
}

fn foreground_window_control_target() -> Result<(HWND, ApprovedFrontmostWindowGuard)> {
    let (hwnd, pid) = foreground_window()?;
    // SAFETY: hwnd is the current foreground top-level window and these calls only inspect its
    // current visibility, minimized/maximized state, title, and rectangle.
    if unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { IsIconic(hwnd) } != 0 {
        return Err(anyhow!(
            "Windows foreground window is not visibly controllable"
        ));
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0
        || rect.right <= rect.left
        || rect.bottom <= rect.top
    {
        return Err(anyhow!("Windows foreground window geometry is invalid"));
    }
    let application = process_name(pid)?;
    let target = ApprovedFrontmostWindowGuard {
        platform: "windows".to_string(),
        application,
        pid,
        window_id: format!("0x{:x}", hwnd as usize),
        position: [f64::from(rect.left), f64::from(rect.top)],
        size: [
            f64::from(rect.right.saturating_sub(rect.left)),
            f64::from(rect.bottom.saturating_sub(rect.top)),
        ],
        fullscreen: None,
        maximized: Some(unsafe { IsZoomed(hwnd) != 0 }),
        position_settable: true,
        size_settable: true,
        fullscreen_settable: false,
    };
    target.validate()?;
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(anyhow!(
            "Windows foreground window changed during control-target discovery"
        ));
    }
    Ok((hwnd, target))
}

pub(super) fn frontmost_window_control_target() -> Result<ApprovedFrontmostWindowGuard> {
    foreground_window_control_target().map(|(_, target)| target)
}

fn same_window_identity(
    left: &ApprovedFrontmostWindowGuard,
    right: &ApprovedFrontmostWindowGuard,
) -> bool {
    left.platform == right.platform
        && left.application == right.application
        && left.pid == right.pid
        && left.window_id == right.window_id
}

fn same_approved_window_snapshot(
    current: &ApprovedFrontmostWindowGuard,
    approved: &ApprovedFrontmostWindowGuard,
) -> bool {
    same_window_identity(current, approved)
        && current.position == approved.position
        && current.size == approved.size
        && current.maximized == approved.maximized
        && current.position_settable == approved.position_settable
        && current.size_settable == approved.size_settable
        && current.fullscreen_settable == approved.fullscreen_settable
}

fn virtual_desktop_rect() -> Result<RECT> {
    // SAFETY: GetSystemMetrics reads the current interactive desktop metrics without retaining
    // pointers or mutating desktop state.
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if width <= 0 || height <= 0 {
        return Err(anyhow!("Windows virtual desktop geometry is invalid"));
    }
    let right = left
        .checked_add(width)
        .ok_or_else(|| anyhow!("Windows virtual desktop width overflowed"))?;
    let bottom = top
        .checked_add(height)
        .ok_or_else(|| anyhow!("Windows virtual desktop height overflowed"))?;
    Ok(RECT {
        left,
        top,
        right,
        bottom,
    })
}

fn intersect_rect(left: RECT, right: RECT) -> Option<RECT> {
    let intersection = RECT {
        left: left.left.max(right.left),
        top: left.top.max(right.top),
        right: left.right.min(right.right),
        bottom: left.bottom.min(right.bottom),
    };
    (intersection.right > intersection.left && intersection.bottom > intersection.top)
        .then_some(intersection)
}

fn rect_equals(left: &RECT, right: &RECT) -> bool {
    left.left == right.left
        && left.top == right.top
        && left.right == right.right
        && left.bottom == right.bottom
}

fn bounded_bstr(value: ::windows::core::BSTR, maximum_chars: usize) -> String {
    String::from_utf16_lossy(&value)
        .chars()
        .take(maximum_chars)
        .map(|character| {
            if is_unsafe_typed_character(character) {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn control_type_name(control_type: i32) -> &'static str {
    match control_type {
        50000 => "button",
        50001 => "calendar",
        50002 => "check_box",
        50003 => "combo_box",
        50004 => "edit",
        50005 => "hyperlink",
        50006 => "image",
        50007 => "list_item",
        50008 => "list",
        50009 => "menu",
        50010 => "menu_bar",
        50011 => "menu_item",
        50012 => "progress_bar",
        50013 => "radio_button",
        50014 => "scroll_bar",
        50015 => "slider",
        50016 => "spinner",
        50017 => "status_bar",
        50018 => "tab",
        50019 => "tab_item",
        50020 => "text",
        50021 => "tool_bar",
        50022 => "tool_tip",
        50023 => "tree",
        50024 => "tree_item",
        50025 => "custom",
        50026 => "group",
        50027 => "thumb",
        50028 => "data_grid",
        50029 => "data_item",
        50030 => "document",
        50031 => "split_button",
        50032 => "window",
        50033 => "pane",
        50034 => "header",
        50035 => "header_item",
        50036 => "table",
        50037 => "title_bar",
        50038 => "separator",
        50039 => "semantic_zoom",
        50040 => "app_bar",
        _ => "unknown",
    }
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: lparam is created from a live WindowEnumeration immediately above and EnumWindows is
    // synchronous. The callback never stores the borrowed pointer.
    let context = unsafe { &mut *(lparam as *mut WindowEnumeration) };
    if context.windows.len() >= context.maximum_windows || unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    let title = unsafe { window_title(hwnd) };
    if title.is_empty() {
        return 1;
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return 1;
    }
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width <= 0 || height <= 0 {
        return 1;
    }
    let mut pid = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return 1;
    }
    context.windows.push(WindowRecord {
        pid,
        title,
        position: [rect.left, rect.top],
        size: [width, height],
        frontmost: hwnd == unsafe { GetForegroundWindow() },
    });
    1
}

unsafe extern "system" fn collect_layout_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: lparam points to a live WindowLayoutEnumeration for this synchronous EnumWindows
    // callback. All retained fields are copied into owned Rust values.
    let context = unsafe { &mut *(lparam as *mut WindowLayoutEnumeration) };
    if context.truncated {
        return 1;
    }
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    if !unsafe { is_ordinary_layout_window(hwnd) } {
        context.excluded_window_count = context.excluded_window_count.saturating_add(1);
        return 1;
    }
    let title = unsafe { window_title(hwnd) };
    if title.is_empty() {
        context.excluded_window_count = context.excluded_window_count.saturating_add(1);
        return 1;
    }
    if unsafe { IsIconic(hwnd) } != 0 || unsafe { IsZoomed(hwnd) } != 0 {
        context.excluded_window_count = context.excluded_window_count.saturating_add(1);
        return 1;
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        context.excluded_window_count = context.excluded_window_count.saturating_add(1);
        return 1;
    }
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width < 64 || height < 64 {
        context.excluded_window_count = context.excluded_window_count.saturating_add(1);
        return 1;
    }
    let mut pid = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    let Ok((application, process_identity)) = process_image_identity(pid) else {
        context.excluded_window_count = context.excluded_window_count.saturating_add(1);
        return 1;
    };
    if context.windows.len() >= context.maximum_windows {
        context.truncated = true;
        return 1;
    }
    context.windows.push(ApprovedWindowLayoutGuard {
        platform: "windows".to_string(),
        application,
        process_identity,
        pid,
        window_id: format!("0x{:x}", hwnd as usize),
        position: [f64::from(rect.left), f64::from(rect.top)],
        size: [f64::from(width), f64::from(height)],
    });
    1
}

unsafe fn is_ordinary_layout_window(hwnd: HWND) -> bool {
    // SAFETY: these calls query only style and owner metadata for the borrowed EnumWindows/top-level
    // HWND. Stale handles return zero-like values and fail the required caption check.
    if !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null() {
        return false;
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as usize;
    let extended_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as usize;
    style & WS_CAPTION as usize == WS_CAPTION as usize
        && extended_style & WS_EX_TOOLWINDOW as usize == 0
}

unsafe fn window_title(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) }.clamp(0, 500) as usize;
    if length == 0 {
        return String::new();
    }
    let mut buffer = vec![0_u16; length + 1];
    let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if copied <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..copied as usize])
}

fn process_image_identity(pid: u32) -> Result<(String, String)> {
    // SAFETY: OpenProcess receives a bounded PID and requests query-only access. The returned handle
    // is closed on every path before the owned UTF-16 buffer is decoded.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err(anyhow!("Windows application process is unavailable"));
    }
    let mut buffer = vec![0_u16; MAX_PROCESS_IMAGE_UNITS];
    let mut length = buffer.len() as u32;
    let queried =
        unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) };
    // SAFETY: handle is an owned process handle returned by OpenProcess.
    unsafe { CloseHandle(handle) };
    if queried == 0 || length == 0 || length as usize > buffer.len() {
        return Err(anyhow!("Windows application identity lookup failed"));
    }
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    let application = Path::new(path.as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("Windows application identity is not valid Unicode"))?;
    let application = application.chars().take(120).collect::<String>();
    let normalized_path = path.to_lowercase();
    let process_identity = format!(
        "image-sha256:{}",
        hex::encode(Sha256::digest(normalized_path.as_bytes()))
    );
    Ok((application, process_identity))
}

fn process_name(pid: u32) -> Result<String> {
    process_image_identity(pid).map(|(application, _)| application)
}

fn parse_layout_hwnd(window_id: &str) -> Result<HWND> {
    let value = window_id
        .strip_prefix("0x")
        .and_then(|value| usize::from_str_radix(value, 16).ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| anyhow!("approved Windows layout HWND identity is invalid"))?;
    Ok(value as HWND)
}

fn live_layout_window(approved: &ApprovedWindowLayoutGuard) -> Result<LiveLayoutWindow> {
    approved.validate()?;
    if approved.platform != "windows" {
        return Err(anyhow!("approved window layout platform is not Windows"));
    }
    let hwnd = parse_layout_hwnd(approved.window_id.as_str())?;
    let mut pid = 0_u32;
    // SAFETY: all calls query only the approved borrowed top-level HWND. A stale or invalid handle
    // yields mismatching/empty state and fails closed before mutation.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != approved.pid
        || unsafe { IsWindowVisible(hwnd) } == 0
        || !unsafe { is_ordinary_layout_window(hwnd) }
        || unsafe { IsIconic(hwnd) } != 0
        || unsafe { IsZoomed(hwnd) } != 0
        || unsafe { window_title(hwnd) }.is_empty()
    {
        return Err(anyhow!(
            "approved Windows layout window identity or ordinary-window state changed"
        ));
    }
    let (application, process_identity) = process_image_identity(pid)?;
    if application != approved.application || process_identity != approved.process_identity {
        return Err(anyhow!("approved Windows layout process identity changed"));
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return Err(anyhow!("approved Windows layout geometry is unavailable"));
    }
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width < 64 || height < 64 {
        return Err(anyhow!("approved Windows layout geometry is invalid"));
    }
    Ok(LiveLayoutWindow {
        hwnd,
        position: [rect.left, rect.top],
        size: [width, height],
    })
}

fn layout_guard_geometry_i32(approved: &ApprovedWindowLayoutGuard) -> Option<(i32, i32, i32, i32)> {
    let values = [
        approved.position[0],
        approved.position[1],
        approved.size[0],
        approved.size[1],
    ];
    if values
        .iter()
        .any(|value| !value.is_finite() || value.fract() != 0.0)
    {
        return None;
    }
    Some((
        i32::try_from(values[0] as i64).ok()?,
        i32::try_from(values[1] as i64).ok()?,
        i32::try_from(values[2] as i64).ok()?,
        i32::try_from(values[3] as i64).ok()?,
    ))
}

fn rollback_layout_windows(
    snapshot: &WindowLayoutSnapshot,
    before: &[LiveLayoutWindow],
    applied: &[usize],
) -> Value {
    let mut restored_count = 0usize;
    let mut skipped_count = 0usize;
    let mut failed_count = 0usize;
    for index in applied.iter().rev().copied() {
        let target = &snapshot.windows[index];
        let Some((x, y, width, height)) = layout_guard_geometry_i32(target) else {
            failed_count = failed_count.saturating_add(1);
            continue;
        };
        let Ok(current) = live_layout_window(target) else {
            skipped_count = skipped_count.saturating_add(1);
            continue;
        };
        if current.hwnd != before[index].hwnd
            || current.position != [x, y]
            || current.size != [width, height]
        {
            skipped_count = skipped_count.saturating_add(1);
            continue;
        }
        // SAFETY: current remains the exact approved HWND and still has this batch's target
        // geometry. Restore is limited to the geometry captured immediately before this batch.
        let restored = unsafe {
            SetWindowPos(
                current.hwnd,
                null_mut(),
                before[index].position[0],
                before[index].position[1],
                before[index].size[0],
                before[index].size[1],
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            )
        } != 0;
        let exact = restored
            && live_layout_window(target).ok().is_some_and(|after| {
                after.hwnd == before[index].hwnd
                    && after.position == before[index].position
                    && after.size == before[index].size
            });
        if exact {
            restored_count = restored_count.saturating_add(1);
        } else {
            failed_count = failed_count.saturating_add(1);
        }
    }
    json!({
        "attempted": !applied.is_empty(),
        "restored_count": restored_count,
        "skipped_count": skipped_count,
        "failed_count": failed_count,
        "complete": restored_count == applied.len(),
    })
}

fn window_layout_failure_result(
    snapshot: &WindowLayoutSnapshot,
    before: &[LiveLayoutWindow],
    applied: &[usize],
    reason: &str,
    partial_window_index: Option<usize>,
) -> Value {
    let recovery = rollback_layout_windows(snapshot, before, applied);
    let complete = recovery.get("complete").and_then(Value::as_bool) == Some(true);
    json!({
        "success": false,
        "mode": "approved_input",
        "action": "restore_window_layout",
        "platform": "windows",
        "snapshot_id": snapshot.snapshot_id,
        "snapshot_sha256": snapshot.snapshot_sha256,
        "target_window_count": snapshot.windows.len(),
        "applied_window_count": applied.len(),
        "target_layout_retained": false,
        "action_already_executed": !applied.is_empty() || partial_window_index.is_some(),
        "automatic_replay_safe": false,
        "failure_reason": reason,
        "partial_window_index": partial_window_index,
        "window_layout_recovery": recovery,
        "application_content_rollback": false,
        "manual_review_required": partial_window_index.is_some() || !complete,
    })
}

pub(super) fn lookup_application(pid: u32) -> Result<Value> {
    let application = process_name(pid)?;
    if find_window_for_pid(pid).is_null() {
        return Err(anyhow!(
            "The requested Windows application has no visible top-level window"
        ));
    }
    Ok(json!({
        "application": application,
        "pid": pid,
        "running": true,
        "platform": "windows",
    }))
}

#[derive(Debug, Clone)]
pub(super) struct ApplicationActivationRollbackGuard {
    previous_pid: u32,
    previous_application: String,
    previous_hwnd: usize,
    target_pid: u32,
    target_application: String,
    target_hwnd: usize,
    target_was_minimized: bool,
    changed_foreground_window: bool,
}

pub(super) fn activate_application_with_rollback(
    pid: u32,
    approved_application: String,
    action_cancelled: Option<&AtomicBool>,
) -> Result<(Value, ApplicationActivationRollbackGuard)> {
    let actual_application = process_name(pid)?;
    if actual_application != approved_application {
        return Err(anyhow!(
            "The approved Windows application identity changed before activation"
        ));
    }
    let hwnd = find_window_for_pid(pid);
    if hwnd.is_null() {
        return Err(anyhow!(
            "The requested Windows application has no visible top-level window"
        ));
    }
    ensure_action_not_cancelled(action_cancelled)?;
    let (previous_hwnd, previous_pid) = foreground_window()?;
    let previous_application = process_name(previous_pid)?;
    if unsafe { GetForegroundWindow() } != previous_hwnd {
        return Err(anyhow!(
            "The Windows foreground application changed before activation"
        ));
    }
    if !window_matches_identity(hwnd, pid, approved_application.as_str())? {
        return Err(anyhow!(
            "The approved Windows application window identity changed before activation"
        ));
    }
    let target_was_minimized = unsafe { IsIconic(hwnd) != 0 };
    // SAFETY: hwnd came from a current synchronous EnumWindows pass. Restoring a non-minimized
    // window is harmless; Windows itself enforces foreground-stealing and integrity restrictions.
    unsafe {
        if target_was_minimized {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() != hwnd {
            SetForegroundWindow(hwnd);
        }
        if GetForegroundWindow() != hwnd {
            if target_was_minimized {
                ShowWindow(
                    hwnd,
                    windows_sys::Win32::UI::WindowsAndMessaging::SW_MINIMIZE,
                );
            }
            return Err(anyhow!(
                "Windows refused to activate the approved application under the current foreground policy"
            ));
        }
    }
    Ok((
        json!({
            "success": true,
            "mode": "approved_input",
            "action": "activate_application",
            "application": actual_application,
            "pid": pid,
            "activated": true,
            "platform": "windows",
        }),
        ApplicationActivationRollbackGuard {
            previous_pid,
            previous_application,
            previous_hwnd: previous_hwnd as usize,
            target_pid: pid,
            target_application: approved_application,
            target_hwnd: hwnd as usize,
            target_was_minimized,
            changed_foreground_window: previous_hwnd != hwnd,
        },
    ))
}

pub(super) fn rollback_application_activation(
    guard: &ApplicationActivationRollbackGuard,
) -> Result<Value> {
    if !guard.changed_foreground_window {
        return Ok(application_rollback_result(
            false,
            true,
            "activation_did_not_change_frontmost_application",
            guard,
            false,
        ));
    }
    let previous_hwnd = guard.previous_hwnd as HWND;
    let target_hwnd = guard.target_hwnd as HWND;
    // SAFETY: both handles originated from synchronous foreground/EnumWindows discovery. Every
    // identity is revalidated immediately before the best-effort restore, and no operation is
    // attempted if the user or another application has already changed the foreground window.
    unsafe {
        if GetForegroundWindow() != target_hwnd {
            return Ok(application_rollback_result(
                false,
                false,
                "foreground_changed_after_activation",
                guard,
                false,
            ));
        }
    }
    if !window_matches_identity(
        target_hwnd,
        guard.target_pid,
        guard.target_application.as_str(),
    )? || !window_matches_identity(
        previous_hwnd,
        guard.previous_pid,
        guard.previous_application.as_str(),
    )? {
        return Ok(application_rollback_result(
            false,
            false,
            "previous_application_identity_unavailable",
            guard,
            false,
        ));
    }
    unsafe {
        if SetForegroundWindow(previous_hwnd) == 0 || GetForegroundWindow() != previous_hwnd {
            return Ok(application_rollback_result(
                true,
                false,
                "platform_refused_restore",
                guard,
                false,
            ));
        }
    }
    let mut target_minimized_state_restored = !guard.target_was_minimized;
    if guard.target_was_minimized {
        // SAFETY: target_hwnd was revalidated above and the previous foreground window has already
        // been restored. Re-minimizing only reverses the SW_RESTORE performed by this action.
        unsafe {
            ShowWindow(
                target_hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_MINIMIZE,
            );
            target_minimized_state_restored = IsIconic(target_hwnd) != 0;
        }
    }
    if !target_minimized_state_restored {
        return Ok(application_rollback_result(
            true,
            false,
            "platform_refused_restore",
            guard,
            false,
        ));
    }
    Ok(application_rollback_result(
        true,
        true,
        "cancelled_activation_restored",
        guard,
        target_minimized_state_restored,
    ))
}

fn window_matches_identity(
    hwnd: HWND,
    expected_pid: u32,
    expected_application: &str,
) -> Result<bool> {
    if hwnd.is_null() || unsafe { IsWindowVisible(hwnd) } == 0 {
        return Ok(false);
    }
    let mut actual_pid = 0_u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut actual_pid);
    }
    if actual_pid != expected_pid {
        return Ok(false);
    }
    Ok(process_name(actual_pid)? == expected_application)
}

fn application_rollback_result(
    attempted: bool,
    restored: bool,
    reason: &str,
    guard: &ApplicationActivationRollbackGuard,
    target_minimized_state_restored: bool,
) -> Value {
    json!({
        "scope": "frontmost_application_activation_only",
        "rollback_on_in_flight_cancel": true,
        "attempted": attempted,
        "restored": restored,
        "reason": reason,
        "previous_pid": guard.previous_pid,
        "target_pid": guard.target_pid,
        "target_minimized_state_restored": target_minimized_state_restored,
        "application_content_rollback": false,
        "window_geometry_rollback": false,
    })
}

struct WindowLookup {
    pid: u32,
    hwnd: HWND,
}

fn find_window_for_pid(pid: u32) -> HWND {
    let mut lookup = WindowLookup {
        pid,
        hwnd: null_mut(),
    };
    // SAFETY: the callback borrows lookup only during synchronous enumeration.
    unsafe {
        EnumWindows(
            Some(find_process_window),
            (&mut lookup as *mut WindowLookup) as LPARAM,
        );
    }
    lookup.hwnd
}

unsafe extern "system" fn find_process_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: lparam points to a live WindowLookup for the synchronous EnumWindows call.
    let lookup = unsafe { &mut *(lparam as *mut WindowLookup) };
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    let mut pid = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != lookup.pid || unsafe { window_title(hwnd) }.is_empty() {
        return 1;
    }
    lookup.hwnd = hwnd;
    0
}

pub(super) fn active_displays() -> Result<Vec<DisplayTarget>> {
    let mut monitors = Vec::<(DisplayTarget, String)>::new();
    // SAFETY: the callback borrows monitors only for this synchronous enumeration and copies
    // monitor geometry/device identifiers into owned Rust values.
    let completed = unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect_monitor),
            (&mut monitors as *mut Vec<(DisplayTarget, String)>) as LPARAM,
        )
    };
    if completed == 0 || monitors.is_empty() {
        return Err(anyhow!("Windows reported no usable active displays"));
    }
    monitors.sort_by(|left, right| {
        right
            .0
            .is_main
            .cmp(&left.0.is_main)
            .then_with(|| left.0.origin_x.total_cmp(&right.0.origin_x))
            .then_with(|| left.0.origin_y.total_cmp(&right.0.origin_y))
            .then_with(|| left.1.cmp(&right.1))
    });
    let mut displays = monitors
        .into_iter()
        .map(|(display, _)| display)
        .collect::<Vec<_>>();
    for (offset, display) in displays.iter_mut().enumerate() {
        display.index = (offset + 1) as u32;
    }
    if !displays.first().is_some_and(|display| display.is_main) {
        return Err(anyhow!("Windows primary display is unavailable"));
    }
    Ok(displays)
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _monitor_dc: HDC,
    _monitor_rect: *mut RECT,
    lparam: LPARAM,
) -> i32 {
    // SAFETY: lparam points to a live vector for the synchronous EnumDisplayMonitors call.
    let monitors = unsafe { &mut *(lparam as *mut Vec<(DisplayTarget, String)>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) } == 0 {
        return 1;
    }
    let rect = info.monitorInfo.rcMonitor;
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width <= 0 || height <= 0 {
        return 1;
    }
    let device_length = info
        .szDevice
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(info.szDevice.len());
    let device = String::from_utf16_lossy(&info.szDevice[..device_length]);
    let digest = Sha256::digest(device.as_bytes());
    let mut display_id = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    if display_id == 0 {
        display_id = 1;
    }
    monitors.push((
        DisplayTarget {
            index: 0,
            display_id,
            is_main: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            origin_x: f64::from(rect.left),
            origin_y: f64::from(rect.top),
            width: f64::from(width),
            height: f64::from(height),
            pixels_wide: width as usize,
            pixels_high: height as usize,
            rotation_degrees: 0.0,
        },
        device,
    ));
    1
}

pub(super) fn capture_display(display: &DisplayTarget) -> Result<Value> {
    let width = checked_dimension(display.width, "width")?;
    let height = checked_dimension(display.height, "height")?;
    let png = capture_region_png(
        display.origin_x.round() as i32,
        display.origin_y.round() as i32,
        width,
        height,
    )?;
    screenshot_result(png.as_slice(), display)
}

pub(super) fn capture_frontmost_window() -> Result<Value> {
    let before = foreground_window_capture_identity()?;
    let width = before
        .capture_rect
        .right
        .saturating_sub(before.capture_rect.left);
    let height = before
        .capture_rect
        .bottom
        .saturating_sub(before.capture_rect.top);
    let png = capture_region_png(
        before.capture_rect.left,
        before.capture_rect.top,
        width,
        height,
    )?;
    let after = foreground_window_capture_identity()?;
    if !before.same_identity_and_geometry(&after) {
        return Err(anyhow!(
            "Windows foreground window identity or geometry changed during capture"
        ));
    }
    frontmost_window_screenshot_result(png.as_slice(), &before.capture_target())
}

pub(super) fn set_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    approved.validate()?;
    if approved.platform != "windows"
        || approved.maximized != Some(false)
        || !approved.position_settable
        || !approved.size_settable
    {
        return Err(anyhow!(
            "approved Windows foreground window is not safely movable and resizable"
        ));
    }
    ensure_action_not_cancelled(action_cancelled)?;
    let (hwnd, current) = foreground_window_control_target()?;
    if !same_approved_window_snapshot(&current, &approved) {
        return Err(anyhow!(
            "approved Windows foreground HWND identity, state, or geometry changed before bounds control"
        ));
    }
    // SAFETY: hwnd is the currently revalidated foreground top-level window. The request is
    // bounded and already intersects an active display. The flags preserve z-order and focus.
    let applied = unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            request.x,
            request.y,
            request.width,
            request.height,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
        )
    } != 0;
    let after = foreground_window_control_target()
        .ok()
        .map(|(_, target)| target);
    let target_applied = applied
        && after.as_ref().is_some_and(|target| {
            same_window_identity(target, &approved)
                && target.maximized == Some(false)
                && target.position == [f64::from(request.x), f64::from(request.y)]
                && target.size == [f64::from(request.width), f64::from(request.height)]
        });
    let cancelled =
        action_cancelled.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst));
    if !target_applied || cancelled {
        let recovery = restore_window_bounds(hwnd, &approved);
        return Ok(json!({
            "success": false,
            "mode": "approved_input",
            "action": "set_frontmost_window_bounds",
            "platform": "windows",
            "application": approved.application,
            "pid": approved.pid,
            "window_id": approved.window_id,
            "target_geometry_applied": false,
            "action_already_executed": true,
            "automatic_replay_safe": false,
            "failure_reason": if cancelled { "cancelled_after_action" } else { "target_geometry_readback_mismatch" },
            "window_geometry_recovery": recovery,
        }));
    }
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "set_frontmost_window_bounds",
        "platform": "windows",
        "application": approved.application,
        "pid": approved.pid,
        "window_id": approved.window_id,
        "original_position": approved.position,
        "original_size": approved.size,
        "position": [request.x, request.y],
        "size": [request.width, request.height],
        "target_geometry_applied": true,
        "identity_and_geometry_revalidated_after_action": true,
        "window_geometry_recovery": {
            "attempted": false,
            "restored": false,
            "reason": "action_completed",
        },
    }))
}

fn restore_window_bounds(hwnd: HWND, approved: &ApprovedFrontmostWindowGuard) -> Value {
    let Ok((current_hwnd, current)) = foreground_window_control_target() else {
        return json!({
            "attempted": false,
            "restored": false,
            "reason": "foreground_or_identity_changed",
        });
    };
    if current_hwnd != hwnd || !same_window_identity(&current, approved) {
        return json!({
            "attempted": false,
            "restored": false,
            "reason": "foreground_or_identity_changed",
        });
    }
    let Some((x, y, width, height)) = guard_geometry_i32(approved) else {
        return json!({
            "attempted": false,
            "restored": false,
            "reason": "approved_geometry_invalid",
        });
    };
    // SAFETY: hwnd is still the exact foreground identity approved for this action; flags preserve
    // focus and z-order while restoring only the approved original geometry.
    let restored_call = unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
        )
    } != 0;
    let exact = restored_call
        && foreground_window_control_target()
            .ok()
            .map(|(_, target)| target)
            .is_some_and(|target| same_approved_window_snapshot(&target, approved));
    json!({
        "attempted": true,
        "restored": exact,
        "reason": if exact { "original_geometry_restored" } else { "restore_readback_mismatch" },
    })
}

pub(super) fn rollback_frontmost_window_bounds(
    request: WindowBoundsRequest,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    let Ok((hwnd, current)) = foreground_window_control_target() else {
        return json!({"attempted": false, "restored": false, "reason": "foreground_or_identity_changed"});
    };
    if !same_window_identity(&current, approved)
        || current.maximized != Some(false)
        || current.position != [f64::from(request.x), f64::from(request.y)]
        || current.size != [f64::from(request.width), f64::from(request.height)]
    {
        return json!({
            "attempted": false,
            "restored": false,
            "reason": "foreground_identity_or_target_geometry_changed",
        });
    }
    restore_window_bounds(hwnd, approved)
}

fn guard_geometry_i32(approved: &ApprovedFrontmostWindowGuard) -> Option<(i32, i32, i32, i32)> {
    let values = [
        approved.position[0],
        approved.position[1],
        approved.size[0],
        approved.size[1],
    ];
    if values
        .iter()
        .any(|value| !value.is_finite() || value.fract() != 0.0)
    {
        return None;
    }
    Some((
        i32::try_from(values[0] as i64).ok()?,
        i32::try_from(values[1] as i64).ok()?,
        i32::try_from(values[2] as i64).ok()?,
        i32::try_from(values[3] as i64).ok()?,
    ))
}

pub(super) fn set_frontmost_window_maximized(
    maximized: bool,
    approved: ApprovedFrontmostWindowGuard,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    approved.validate()?;
    if approved.platform != "windows" || approved.maximized == Some(maximized) {
        return Err(anyhow!(
            "approved Windows foreground window maximize transition is unavailable"
        ));
    }
    ensure_action_not_cancelled(action_cancelled)?;
    let (hwnd, current) = foreground_window_control_target()?;
    if !same_approved_window_snapshot(&current, &approved) {
        return Err(anyhow!(
            "approved Windows foreground HWND identity, state, or geometry changed before maximize control"
        ));
    }
    // SAFETY: hwnd is the exact current foreground HWND. ShowWindow performs the standard Windows
    // maximize/restore transition and remains subject to system and integrity policy.
    unsafe {
        ShowWindow(hwnd, if maximized { SW_MAXIMIZE } else { SW_RESTORE });
    }
    let mut target_applied = false;
    for _ in 0..20 {
        if let Ok((current_hwnd, target)) = foreground_window_control_target() {
            if current_hwnd != hwnd || !same_window_identity(&target, &approved) {
                break;
            }
            if target.maximized == Some(maximized) {
                target_applied = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    let cancelled =
        action_cancelled.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst));
    if !target_applied || cancelled {
        let recovery = restore_window_maximized_state(hwnd, &approved);
        return Ok(json!({
            "success": false,
            "mode": "approved_input",
            "action": "set_frontmost_window_maximized",
            "platform": "windows",
            "application": approved.application,
            "pid": approved.pid,
            "window_id": approved.window_id,
            "target_maximized_applied": false,
            "action_already_executed": true,
            "automatic_replay_safe": false,
            "failure_reason": if cancelled { "cancelled_after_action" } else { "target_state_readback_mismatch" },
            "window_state_recovery": recovery,
        }));
    }
    let after = foreground_window_control_target()?.1;
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "set_frontmost_window_maximized",
        "platform": "windows",
        "application": approved.application,
        "pid": approved.pid,
        "window_id": approved.window_id,
        "original_maximized": approved.maximized,
        "maximized": maximized,
        "position": after.position,
        "size": after.size,
        "target_maximized_applied": true,
        "identity_and_state_revalidated_after_action": true,
        "window_state_recovery": {
            "attempted": false,
            "restored": false,
            "reason": "action_completed",
        },
    }))
}

fn restore_window_maximized_state(hwnd: HWND, approved: &ApprovedFrontmostWindowGuard) -> Value {
    let Ok((current_hwnd, current)) = foreground_window_control_target() else {
        return json!({"attempted": false, "restored": false, "reason": "foreground_or_identity_changed"});
    };
    if current_hwnd != hwnd || !same_window_identity(&current, approved) {
        return json!({"attempted": false, "restored": false, "reason": "foreground_or_identity_changed"});
    }
    let original_maximized = approved.maximized.unwrap_or(false);
    // SAFETY: hwnd remains the exact approved foreground identity. This restores only the approved
    // standard maximize state; normal geometry is separately restored below when applicable.
    unsafe {
        ShowWindow(
            hwnd,
            if original_maximized {
                SW_MAXIMIZE
            } else {
                SW_RESTORE
            },
        );
    }
    if !original_maximized {
        if let Some((x, y, width, height)) = guard_geometry_i32(approved) {
            unsafe {
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
                );
            }
        }
    }
    for _ in 0..20 {
        if foreground_window_control_target()
            .ok()
            .map(|(_, target)| target)
            .is_some_and(|target| {
                same_window_identity(&target, approved)
                    && target.maximized == approved.maximized
                    && (original_maximized
                        || (target.position == approved.position && target.size == approved.size))
            })
        {
            return json!({"attempted": true, "restored": true, "reason": "original_window_state_restored"});
        }
        thread::sleep(Duration::from_millis(25));
    }
    json!({"attempted": true, "restored": false, "reason": "restore_readback_mismatch"})
}

pub(super) fn rollback_frontmost_window_maximized(
    maximized: bool,
    approved: &ApprovedFrontmostWindowGuard,
) -> Value {
    let Ok((hwnd, current)) = foreground_window_control_target() else {
        return json!({"attempted": false, "restored": false, "reason": "foreground_or_identity_changed"});
    };
    if !same_window_identity(&current, approved) || current.maximized != Some(maximized) {
        return json!({
            "attempted": false,
            "restored": false,
            "reason": "foreground_identity_or_target_state_changed",
        });
    }
    restore_window_maximized_state(hwnd, approved)
}

fn capture_region_png(left: i32, top: i32, width: i32, height: i32) -> Result<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return Err(anyhow!("Windows screenshot region geometry is invalid"));
    }
    let raw_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= MAX_CAPTURE_RAW_BYTES)
        .ok_or_else(|| anyhow!("Windows screenshot exceeds the bounded raw pixel limit"))?;

    // SAFETY: all GDI handles are checked before use and owned by CaptureHandles, which restores
    // the selected object and releases every acquired handle on all return paths.
    let mut handles = unsafe { CaptureHandles::new(width, height)? };
    let copied = unsafe {
        BitBlt(
            handles.memory_dc,
            0,
            0,
            width,
            height,
            handles.screen_dc,
            left,
            top,
            SRCCOPY | CAPTUREBLT,
        )
    };
    if copied == 0 {
        return Err(anyhow!("Windows screenshot capture failed"));
    }
    handles.restore_selection();

    let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: raw_bytes as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0_u8; raw_bytes];
    // SAFETY: bgra has exactly width*height*4 bytes and bitmap_info requests a top-down 32-bit DIB.
    let scanlines = unsafe {
        GetDIBits(
            handles.screen_dc,
            handles.bitmap,
            0,
            height as u32,
            bgra.as_mut_ptr().cast(),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        )
    };
    if scanlines != height {
        return Err(anyhow!(
            "Windows display capture returned incomplete pixels"
        ));
    }
    encode_png(width as u32, height as u32, &bgra)
}

fn checked_dimension(value: f64, label: &str) -> Result<i32> {
    if !value.is_finite() || value <= 0.0 || value > i32::MAX as f64 {
        return Err(anyhow!("Windows display {label} is invalid"));
    }
    let rounded = value.round() as i32;
    if rounded <= 0 {
        return Err(anyhow!("Windows display {label} is invalid"));
    }
    Ok(rounded)
}

struct CaptureHandles {
    screen_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    previous_object: HGDIOBJ,
}

impl CaptureHandles {
    unsafe fn new(width: i32, height: i32) -> Result<Self> {
        let screen_dc = unsafe { GetDC(null_mut()) };
        if screen_dc.is_null() {
            return Err(anyhow!("Windows desktop device context is unavailable"));
        }
        let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if memory_dc.is_null() {
            unsafe { ReleaseDC(null_mut(), screen_dc) };
            return Err(anyhow!("Windows capture memory context is unavailable"));
        }
        let bitmap = unsafe { CreateCompatibleBitmap(screen_dc, width, height) };
        if bitmap.is_null() {
            unsafe {
                DeleteDC(memory_dc);
                ReleaseDC(null_mut(), screen_dc);
            }
            return Err(anyhow!("Windows capture bitmap allocation failed"));
        }
        let previous_object = unsafe { SelectObject(memory_dc, bitmap as HGDIOBJ) };
        if previous_object.is_null() {
            unsafe {
                DeleteObject(bitmap as HGDIOBJ);
                DeleteDC(memory_dc);
                ReleaseDC(null_mut(), screen_dc);
            }
            return Err(anyhow!("Windows capture bitmap selection failed"));
        }
        Ok(Self {
            screen_dc,
            memory_dc,
            bitmap,
            previous_object,
        })
    }

    fn restore_selection(&mut self) {
        if self.previous_object.is_null() {
            return;
        }
        // SAFETY: previous_object was returned by SelectObject for this memory DC.
        unsafe { SelectObject(self.memory_dc, self.previous_object) };
        self.previous_object = null_mut();
    }
}

impl Drop for CaptureHandles {
    fn drop(&mut self) {
        self.restore_selection();
        // SAFETY: all handles were acquired by CaptureHandles::new and are released exactly once.
        unsafe {
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap as HGDIOBJ);
            }
            if !self.memory_dc.is_null() {
                DeleteDC(self.memory_dc);
            }
            if !self.screen_dc.is_null() {
                ReleaseDC(null_mut(), self.screen_dc);
            }
        }
    }
}

fn encode_png(width: u32, height: u32, bgra: &[u8]) -> Result<Vec<u8>> {
    let row_bytes = (width as usize)
        .checked_mul(3)
        .ok_or_else(|| anyhow!("Windows screenshot row size overflow"))?;
    let raw_capacity = row_bytes
        .checked_add(1)
        .and_then(|row| row.checked_mul(height as usize))
        .ok_or_else(|| anyhow!("Windows screenshot PNG size overflow"))?;
    let mut scanlines = Vec::with_capacity(raw_capacity);
    for row in bgra.chunks_exact((width as usize) * 4) {
        scanlines.push(0);
        for pixel in row.chunks_exact(4) {
            scanlines.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder
        .write_all(scanlines.as_slice())
        .context("compress Windows screenshot PNG")?;
    let compressed = encoder
        .finish()
        .context("finish Windows screenshot PNG compression")?;

    let mut output = Vec::with_capacity(compressed.len() + 96);
    output.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    write_png_chunk(&mut output, b"IHDR", header.as_slice());
    write_png_chunk(&mut output, b"IDAT", compressed.as_slice());
    write_png_chunk(&mut output, b"IEND", &[]);
    Ok(output)
}

fn write_png_chunk(output: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(name);
    output.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(name);
    crc.update(data);
    output.extend_from_slice(&crc.finalize().to_be_bytes());
}

pub(super) fn click(
    action: ClickAction<'_>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    ensure_action_not_cancelled(action_cancelled)?;
    move_pointer(action.global_x, action.global_y)?;
    let (down, up) = if action.button == "right" {
        (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
    } else {
        (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
    };
    for click_index in 1..=action.click_count {
        ensure_action_not_cancelled(action_cancelled)?;
        send_mouse_flags(down, 0)?;
        let mut mouse_up = MouseButtonReleaseGuard::new(up);
        mouse_up.release()?;
        if click_index < action.click_count {
            thread::sleep(DOUBLE_CLICK_INTERVAL);
            ensure_action_not_cancelled(action_cancelled)?;
        }
    }
    Ok(click_result(&action))
}

pub(super) fn drag(action: DragAction, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    ensure_action_not_cancelled(action_cancelled)?;
    move_pointer(action.global_start_x, action.global_start_y)?;
    send_mouse_flags(MOUSEEVENTF_LEFTDOWN, 0)?;
    let mut mouse_up = MouseButtonReleaseGuard::new(MOUSEEVENTF_LEFTUP);
    let steps = drag_step_count(action.duration_ms);
    let interval = Duration::from_millis((action.duration_ms / u64::from(steps)).max(1));
    for step in 1..=steps {
        ensure_action_not_cancelled(action_cancelled)?;
        thread::sleep(interval);
        ensure_action_not_cancelled(action_cancelled)?;
        let progress = f64::from(step) / f64::from(steps);
        move_pointer(
            action.global_start_x + (action.global_end_x - action.global_start_x) * progress,
            action.global_start_y + (action.global_end_y - action.global_start_y) * progress,
        )?;
    }
    move_pointer(action.global_end_x, action.global_end_y)?;
    mouse_up.release()?;
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
        "platform": "windows",
    }))
}

struct MouseButtonReleaseGuard {
    release_flags: u32,
    armed: bool,
}

impl MouseButtonReleaseGuard {
    fn new(release_flags: u32) -> Self {
        Self {
            release_flags,
            armed: true,
        }
    }

    fn release(&mut self) -> Result<()> {
        if self.armed {
            send_mouse_flags(self.release_flags, 0)?;
            self.armed = false;
        }
        Ok(())
    }
}

impl Drop for MouseButtonReleaseGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = send_mouse_flags(self.release_flags, 0);
            self.armed = false;
        }
    }
}

fn move_pointer(global_x: f64, global_y: f64) -> Result<()> {
    // SAFETY: GetSystemMetrics takes fixed enum values and returns the current virtual-desktop
    // bounds without modifying desktop state.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if width <= 1 || height <= 1 {
        return Err(anyhow!("Windows virtual desktop geometry is unavailable"));
    }
    let x = global_x.round() as i32;
    let y = global_y.round() as i32;
    if x < left || y < top || x >= left.saturating_add(width) || y >= top.saturating_add(height) {
        return Err(anyhow!(
            "approved Windows pointer location left the virtual desktop"
        ));
    }
    let normalized_x = (((x - left) as i64 * 65_535) / i64::from(width - 1)) as i32;
    let normalized_y = (((y - top) as i64 * 65_535) / i64::from(height - 1)) as i32;
    send_inputs(&[mouse_input(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        0,
        normalized_x,
        normalized_y,
    )])
}

fn send_mouse_flags(flags: u32, data: u32) -> Result<()> {
    send_inputs(&[mouse_input(flags, data, 0, 0)])
}

fn mouse_input(flags: u32, data: u32, dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub(super) fn press_key(action: KeyAction<'_>) -> Result<Value> {
    let key = windows_key_code(action.key)?;
    let modifiers = action
        .modifiers
        .iter()
        .map(|modifier| windows_modifier_code(modifier))
        .collect::<Result<Vec<_>>>()?;
    let mut inputs = Vec::with_capacity(modifiers.len() * 2 + 2);
    for modifier in &modifiers {
        inputs.push(key_input(*modifier, false));
    }
    inputs.push(key_input(key, false));
    inputs.push(key_input(key, true));
    for modifier in modifiers.iter().rev() {
        inputs.push(key_input(*modifier, true));
    }
    if let Err(error) = send_inputs(inputs.as_slice()) {
        let mut releases = vec![key_input(key, true)];
        releases.extend(
            modifiers
                .iter()
                .rev()
                .map(|modifier| key_input(*modifier, true)),
        );
        let _ = send_inputs(releases.as_slice());
        return Err(error);
    }
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "press_key",
        "key": action.key,
        "modifiers": action.modifiers,
        "platform": "windows",
    }))
}

fn windows_key_code(key: &str) -> Result<u16> {
    match key {
        "enter" => Ok(VK_RETURN),
        "tab" => Ok(VK_TAB),
        "space" => Ok(VK_SPACE),
        "escape" => Ok(VK_ESCAPE),
        "backspace" => Ok(VK_BACK),
        "left" => Ok(VK_LEFT),
        "right" => Ok(VK_RIGHT),
        "up" => Ok(VK_UP),
        "down" => Ok(VK_DOWN),
        "home" => Ok(VK_HOME),
        "end" => Ok(VK_END),
        "page_up" => Ok(VK_PRIOR),
        "page_down" => Ok(VK_NEXT),
        _ => Err(anyhow!("unsupported reviewed Windows key: {key}")),
    }
}

fn windows_modifier_code(modifier: &str) -> Result<u16> {
    match modifier {
        "command" => Ok(VK_LWIN),
        "control" => Ok(VK_LCONTROL),
        "option" => Ok(VK_LMENU),
        "shift" => Ok(VK_LSHIFT),
        _ => Err(anyhow!("unsupported reviewed Windows modifier: {modifier}")),
    }
}

fn key_input(key: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

struct ValidatedTextTarget {
    element: IUIAutomationElement,
    session: UiAutomationSession,
    foreground: HWND,
    pid: u32,
    class: WindowsTextTargetClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsTextTargetClass {
    NativeEdit,
    ContentEditable,
}

impl WindowsTextTargetClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::NativeEdit => "native_edit",
            Self::ContentEditable => "contenteditable",
        }
    }
}

impl ValidatedTextTarget {
    fn validate() -> Result<Self> {
        let (foreground, pid) = foreground_window()?;
        let session = UiAutomationSession::new()?;
        // SAFETY: GetFocusedElement returns an owned UI Automation element for the current desktop
        // focus. Every security-relevant property is read and required below before any input.
        let element = unsafe { session.automation.GetFocusedElement() }
            .context("inspect the focused Windows UI Automation element")?;
        let class = validate_editable_text_element(&element, pid)?;
        Ok(Self {
            element,
            session,
            foreground,
            pid,
            class,
        })
    }

    fn ensure_still_focused(&self) -> Result<()> {
        let (foreground, pid) = foreground_window()?;
        if foreground != self.foreground || pid != self.pid {
            return Err(anyhow!(
                "Windows foreground window changed after text-target validation"
            ));
        }
        // SAFETY: this re-reads the live focused element immediately before SendInput and compares
        // it with the previously validated COM identity. Any failure rejects the text action.
        let current = unsafe { self.session.automation.GetFocusedElement() }
            .context("recheck the focused Windows UI Automation element")?;
        let unchanged = unsafe {
            self.session
                .automation
                .CompareElements(&self.element, &current)
        }
        .context("compare Windows text-target UI Automation identities")?;
        if !unchanged.as_bool() {
            return Err(anyhow!(
                "Windows focused text control changed after validation"
            ));
        }
        let class = validate_editable_text_element(&current, self.pid)?;
        if class != self.class {
            return Err(anyhow!(
                "Windows focused text target class changed after validation"
            ));
        }
        Ok(())
    }
}

fn validate_editable_text_element(
    element: &IUIAutomationElement,
    foreground_pid: u32,
) -> Result<WindowsTextTargetClass> {
    let pid = unsafe { element.CurrentProcessId() }
        .context("read focused Windows control process identity")?;
    if pid <= 0 || pid as u32 != foreground_pid {
        return Err(anyhow!(
            "Windows focused control does not belong to the foreground application"
        ));
    }
    let is_password = unsafe { element.CurrentIsPassword() }
        .context("confirm whether the focused Windows control is a password field")?
        .as_bool();
    if is_password {
        return Err(anyhow!(
            "Computer Use refuses to type into a Windows password field"
        ));
    }
    let enabled = unsafe { element.CurrentIsEnabled() }
        .context("confirm the focused Windows control is enabled")?
        .as_bool();
    let focusable = unsafe { element.CurrentIsKeyboardFocusable() }
        .context("confirm the focused Windows control is keyboard-focusable")?
        .as_bool();
    let focused = unsafe { element.CurrentHasKeyboardFocus() }
        .context("confirm the focused Windows control has keyboard focus")?
        .as_bool();
    let offscreen = unsafe { element.CurrentIsOffscreen() }
        .context("confirm the focused Windows control is visible")?
        .as_bool();
    if !enabled || !focusable || !focused || offscreen {
        return Err(anyhow!(
            "Computer Use Windows text input requires a visible, enabled, keyboard-focused text target"
        ));
    }
    let bounds = unsafe { element.CurrentBoundingRectangle() }
        .context("confirm the focused Windows control bounds")?;
    if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return Err(anyhow!(
            "Computer Use Windows text input requires a visible text target with non-empty bounds"
        ));
    }
    let control_type = unsafe { element.CurrentControlType() }
        .context("confirm the focused Windows control type")?;
    if control_type == UIA_EditControlTypeId {
        let value_pattern =
            unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
                .context("confirm the focused Windows edit control supports ValuePattern")?;
        let read_only = unsafe { value_pattern.CurrentIsReadOnly() }
            .context("confirm the focused Windows edit control is writable")?
            .as_bool();
        if read_only {
            return Err(anyhow!(
                "Computer Use refuses to type into a read-only Windows edit control"
            ));
        }
        return Ok(WindowsTextTargetClass::NativeEdit);
    }
    if control_type == UIA_DocumentControlTypeId
        || control_type == UIA_PaneControlTypeId
        || control_type == UIA_CustomControlTypeId
    {
        let _text_edit_pattern = unsafe {
            element.GetCurrentPatternAs::<IUIAutomationTextEditPattern>(UIA_TextEditPatternId)
        }
        .context(
            "confirm the focused Windows non-Edit text target explicitly supports TextEditPattern",
        )?;
        return Ok(WindowsTextTargetClass::ContentEditable);
    }
    Err(anyhow!(
        "Computer Use Windows text input requires a writable UI Automation Edit control or explicit TextEditPattern contenteditable target"
    ))
}

pub(super) fn type_text(action: TypedTextAction<'_>) -> Result<Value> {
    let target = ValidatedTextTarget::validate()?;
    target.ensure_still_focused()?;
    let mut inputs = Vec::with_capacity(action.utf16.len() * 2);
    for unit in action.utf16.iter().copied() {
        inputs.push(unicode_key_input(unit, false));
        inputs.push(unicode_key_input(unit, true));
    }
    if let Err(error) = send_inputs(inputs.as_slice()) {
        let releases = action
            .utf16
            .iter()
            .copied()
            .map(|unit| unicode_key_input(unit, true))
            .collect::<Vec<_>>();
        let _ = send_inputs(releases.as_slice());
        return Err(error);
    }
    let mut result = typed_text_result(&action);
    result
        .as_object_mut()
        .ok_or_else(|| anyhow!("Computer Use text result serialization failed"))?
        .insert("platform".to_string(), Value::String("windows".to_string()));
    result
        .as_object_mut()
        .ok_or_else(|| anyhow!("Computer Use text result serialization failed"))?
        .insert(
            "target_class".to_string(),
            Value::String(target.class.as_str().to_string()),
        );
    Ok(result)
}

fn unicode_key_input(unit: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE | if key_up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub(super) fn scroll(action: ScrollAction) -> Result<Value> {
    let mut inputs = Vec::with_capacity(2);
    if action.delta_y != 0 {
        inputs.push(mouse_input(MOUSEEVENTF_WHEEL, action.delta_y as u32, 0, 0));
    }
    if action.delta_x != 0 {
        inputs.push(mouse_input(MOUSEEVENTF_HWHEEL, action.delta_x as u32, 0, 0));
    }
    send_inputs(inputs.as_slice())?;
    Ok(json!({
        "success": true,
        "mode": "approved_input",
        "action": "scroll",
        "delta_y": action.delta_y,
        "delta_x": action.delta_x,
        "platform": "windows",
    }))
}

fn send_inputs(inputs: &[INPUT]) -> Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    // SAFETY: inputs is a live contiguous slice of initialized INPUT records for the synchronous
    // SendInput call. Windows enforces desktop/integrity policy before accepting them.
    let inserted = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if inserted != inputs.len() as u32 {
        return Err(anyhow!(
            "Windows accepted only {inserted} of {} approved input events",
            inputs.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_encoder_writes_bounded_truecolor_png() {
        let bgra = [0_u8, 0, 255, 255, 0, 255, 0, 255];
        let png = encode_png(2, 1, &bgra).expect("encode PNG");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.windows(4).any(|chunk| chunk == b"IHDR"));
        assert!(png.windows(4).any(|chunk| chunk == b"IDAT"));
        assert!(png.windows(4).any(|chunk| chunk == b"IEND"));
    }

    #[test]
    fn windows_key_mapping_is_allowlisted() {
        assert_eq!(windows_key_code("enter").unwrap(), VK_RETURN);
        assert_eq!(windows_modifier_code("command").unwrap(), VK_LWIN);
        assert!(windows_key_code("a").is_err());
        assert!(windows_modifier_code("meta").is_err());
    }

    #[test]
    fn ui_automation_control_types_and_unicode_inputs_are_bounded() {
        assert_eq!(control_type_name(UIA_EditControlTypeId.0), "edit");
        assert_eq!(control_type_name(123), "unknown");
        let down = unicode_key_input('A' as u16, false);
        let up = unicode_key_input('A' as u16, true);
        // SAFETY: both INPUT values were initialized with the keyboard union arm immediately above.
        let (down, up) = unsafe { (down.Anonymous.ki, up.Anonymous.ki) };
        assert_eq!(down.wVk, 0);
        assert_eq!(down.wScan, 'A' as u16);
        assert_eq!(down.dwFlags, KEYEVENTF_UNICODE);
        assert_eq!(up.dwFlags, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
    }
}
