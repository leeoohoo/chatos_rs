// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    bounded_integer, bounded_signed_integer, display_approval_argument, finite_number, key_code,
    reject_unknown_fields, resolve_display, DisplayTarget, DEFAULT_DRAG_DURATION_MS,
    MAX_DRAG_DURATION_MS, MAX_DRAG_STEPS, MAX_SCROLL_DELTA, MAX_TYPED_TEXT_CHARS,
    MAX_TYPED_TEXT_UTF16_UNITS, MIN_DRAG_DURATION_MS,
};

#[derive(Debug, Clone)]
pub(super) struct ClickAction<'a> {
    pub(super) display: DisplayTarget,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) global_x: f64,
    pub(super) global_y: f64,
    pub(super) button: &'a str,
    pub(super) click_count: u32,
}

#[derive(Debug)]
pub(super) struct DragAction {
    pub(super) display: DisplayTarget,
    pub(super) start_x: f64,
    pub(super) start_y: f64,
    pub(super) end_x: f64,
    pub(super) end_y: f64,
    pub(super) global_start_x: f64,
    pub(super) global_start_y: f64,
    pub(super) global_end_x: f64,
    pub(super) global_end_y: f64,
    pub(super) duration_ms: u64,
}

#[derive(Debug)]
pub(super) struct KeyAction<'a> {
    pub(super) key: &'a str,
    pub(super) modifiers: Vec<&'a str>,
}

#[derive(Debug)]
pub(super) struct TypedTextAction<'a> {
    pub(super) text: &'a str,
    pub(super) utf16: Vec<u16>,
    pub(super) character_count: usize,
    pub(super) sha256: String,
}

#[derive(Debug)]
pub(super) struct ScrollAction {
    pub(super) delta_y: i32,
    pub(super) delta_x: i32,
}

pub(super) fn parse_click(arguments: &Value) -> Result<ClickAction<'_>> {
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

pub(super) fn parse_click_count(arguments: &Value, button: &str) -> Result<u32> {
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

pub(super) fn click_approval_arguments(action: &ClickAction<'_>) -> Result<Vec<String>> {
    Ok(vec![
        format!("--display-index={}", action.display.index),
        format!("--x={}", action.x),
        format!("--y={}", action.y),
        format!("--button={}", action.button),
        format!("--click-count={}", action.click_count),
        display_approval_argument(&action.display)?,
    ])
}

pub(super) fn parse_drag(arguments: &Value) -> Result<DragAction> {
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

pub(super) fn ensure_action_not_cancelled(action_cancelled: Option<&AtomicBool>) -> Result<()> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        return Err(anyhow!("Computer Use action was cancelled"));
    }
    Ok(())
}

pub(super) fn drag_step_count(duration_ms: u64) -> u32 {
    ((duration_ms.saturating_add(15) / 16) as u32).clamp(4, MAX_DRAG_STEPS)
}

pub(super) fn parse_key_action(arguments: &Value) -> Result<KeyAction<'_>> {
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

pub(super) fn key_confirmation_risk(action: &KeyAction<'_>) -> Option<&'static str> {
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

pub(super) fn parse_typed_text(arguments: &Value) -> Result<TypedTextAction<'_>> {
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

pub(super) fn is_unsafe_typed_character(character: char) -> bool {
    character.is_control()
        || matches!(
            character as u32,
            0x200B..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069 | 0xFEFF
        )
}

pub(super) fn parse_scroll(arguments: &Value) -> Result<ScrollAction> {
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
