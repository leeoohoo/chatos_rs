// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::action::is_unsafe_typed_character;
use super::approval::format_audit_number;
use super::{
    reject_unknown_fields, MAX_WINDOW_COORDINATE, MAX_WINDOW_DIMENSION, MIN_WINDOW_COORDINATE,
    MIN_WINDOW_DIMENSION,
};

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ApprovedFrontmostWindowGuard {
    pub(super) platform: String,
    pub(super) application: String,
    pub(super) pid: u32,
    pub(super) window_id: String,
    pub(super) position: [f64; 2],
    pub(super) size: [f64; 2],
    pub(super) fullscreen: Option<bool>,
    pub(super) maximized: Option<bool>,
    pub(super) position_settable: bool,
    pub(super) size_settable: bool,
    pub(super) fullscreen_settable: bool,
}

impl ApprovedFrontmostWindowGuard {
    pub(super) fn validate(&self) -> Result<()> {
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

    pub(super) fn geometry(&self) -> String {
        format!(
            "{} x {} @ {}, {}",
            format_audit_number(self.size[0]),
            format_audit_number(self.size[1]),
            format_audit_number(self.position[0]),
            format_audit_number(self.position[1]),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WindowBoundsRequest {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

impl WindowBoundsRequest {
    pub(super) fn geometry(&self) -> String {
        format!("{} x {} @ {}, {}", self.width, self.height, self.x, self.y)
    }
}

pub(super) fn parse_window_bounds_request(arguments: &Value) -> Result<WindowBoundsRequest> {
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

pub(super) fn parse_window_fullscreen_request(arguments: &Value) -> Result<bool> {
    reject_unknown_fields(arguments, &["fullscreen"])?;
    arguments
        .get("fullscreen")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("fullscreen must be a boolean"))
}

pub(super) fn parse_window_maximized_request(arguments: &Value) -> Result<bool> {
    reject_unknown_fields(arguments, &["maximized"])?;
    arguments
        .get("maximized")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("maximized must be a boolean"))
}

fn required_bounded_i32(arguments: &Value, field: &str, minimum: i64, maximum: i64) -> Result<i32> {
    let value = arguments
        .get(field)
        .and_then(Value::as_i64)
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or_else(|| anyhow!("{field} must be an integer between {minimum} and {maximum}"))?;
    i32::try_from(value).map_err(|_| anyhow!("{field} is outside the supported integer range"))
}

pub(super) fn validate_window_bounds_capability(
    target: &ApprovedFrontmostWindowGuard,
) -> Result<()> {
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

pub(super) fn validate_window_fullscreen_capability(
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

pub(super) fn validate_window_maximized_capability(
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

pub(super) fn window_approval_argument(target: &ApprovedFrontmostWindowGuard) -> Result<String> {
    target.validate()?;
    Ok(format!("--window-json={}", serde_json::to_string(target)?))
}

pub(super) fn approved_window_guard(
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
