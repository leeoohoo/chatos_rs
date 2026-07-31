// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::{anyhow, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
#[cfg(target_os = "macos")]
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::display::{active_displays, current_platform_name, resolve_display, DisplayTarget};
#[cfg(target_os = "macos")]
use super::{
    classify_macos_screenshot_error, execute_jxa, join_reader, read_limited,
    COMPUTER_USE_COMMAND_TIMEOUT, COMPUTER_USE_STDERR_MAX_BYTES,
    FRONTMOST_WINDOW_CAPTURE_TARGET_JXA, MACOS_SCREENCAPTURE_PATH,
};

const COMPUTER_USE_SCREENSHOT_MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct FrontmostWindowCaptureTarget {
    pub(super) platform: &'static str,
    pub(super) application: String,
    pub(super) pid: u32,
    pub(super) window_id: String,
    pub(super) title: String,
    pub(super) position: [f64; 2],
    pub(super) size: [f64; 2],
    pub(super) capture_position: [f64; 2],
    pub(super) capture_size: [f64; 2],
    pub(super) clipped_to_visible_desktop: bool,
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

#[cfg(target_os = "macos")]
fn macos_frontmost_window_identity() -> Result<MacosFrontmostWindowIdentity> {
    let value = execute_jxa(FRONTMOST_WINDOW_CAPTURE_TARGET_JXA, &[])?;
    let identity = serde_json::from_value::<MacosFrontmostWindowIdentity>(value)
        .context("decode macOS frontmost window capture target")?;
    identity.validate()?;
    Ok(identity)
}

#[cfg(target_os = "macos")]
pub(super) fn capture_frontmost_window() -> Result<Value> {
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
pub(super) fn capture_frontmost_window() -> Result<Value> {
    super::windows::capture_frontmost_window()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn capture_frontmost_window() -> Result<Value> {
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
pub(super) fn capture_display(requested_index: Option<u32>) -> Result<Value> {
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
pub(super) fn capture_display(requested_index: Option<u32>) -> Result<Value> {
    let display = if let Some(index) = requested_index {
        active_displays()?
            .into_iter()
            .find(|display| display.index == index)
            .ok_or_else(|| anyhow!("the selected display is no longer active"))?
    } else {
        resolve_display(None)?
    };
    super::windows::capture_display(&display)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn capture_display(_requested_index: Option<u32>) -> Result<Value> {
    Err(anyhow!(
        "Computer Use screenshots are unsupported on this platform"
    ))
}

pub(super) fn screenshot_result(bytes: &[u8], display: &DisplayTarget) -> Result<Value> {
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

pub(super) fn frontmost_window_screenshot_result(
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
