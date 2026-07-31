// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    reject_unknown_fields, WindowBoundsRequest, MAX_ACTIVE_DISPLAYS, MIN_WINDOW_DIMENSION,
};
#[cfg(target_os = "macos")]
use super::{
    CGDisplayBounds, CGDisplayPixelsHigh, CGDisplayPixelsWide, CGDisplayRotation,
    CGGetActiveDisplayList, CGMainDisplayID,
};

#[derive(Debug, Clone)]
pub(super) struct DisplayTarget {
    pub(super) index: u32,
    pub(super) display_id: u32,
    pub(super) is_main: bool,
    pub(super) origin_x: f64,
    pub(super) origin_y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) pixels_wide: usize,
    pub(super) pixels_high: usize,
    pub(super) rotation_degrees: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(super) struct ApprovedDisplayGuard {
    pub(super) index: u32,
    pub(super) display_id: u32,
    pub(super) is_main: bool,
    pub(super) origin_x: f64,
    pub(super) origin_y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) pixels_wide: usize,
    pub(super) pixels_high: usize,
    pub(super) rotation_degrees: f64,
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

pub(super) fn display_approval_argument(display: &DisplayTarget) -> Result<String> {
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

pub(super) fn validate_approved_display(
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

pub(super) fn active_display_layout_guard() -> Result<Vec<ApprovedDisplayGuard>> {
    active_displays().map(|displays| {
        displays
            .iter()
            .map(ApprovedDisplayGuard::from)
            .collect::<Vec<_>>()
    })
}

pub(super) fn validate_requested_window_bounds_against_layout(
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

pub(super) fn window_display_layout_approval_argument(
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

pub(super) fn approved_window_display_layout(
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

pub(super) fn validate_approved_window_display_layout(
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

pub(super) fn required_display_index(arguments: &Value) -> Result<u32> {
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

#[cfg(target_os = "macos")]
pub(super) fn active_displays() -> Result<Vec<DisplayTarget>> {
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
pub(super) fn active_displays() -> Result<Vec<DisplayTarget>> {
    super::windows::active_displays()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn active_displays() -> Result<Vec<DisplayTarget>> {
    Err(anyhow!(
        "Computer Use display discovery is unsupported on this platform"
    ))
}

pub(super) fn resolve_display(index: Option<&Value>) -> Result<DisplayTarget> {
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

pub(super) fn list_displays() -> Result<Value> {
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

pub(super) fn current_platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unsupported"
    }
}
