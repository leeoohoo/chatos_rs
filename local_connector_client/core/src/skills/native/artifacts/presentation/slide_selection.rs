// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeSet, HashSet};

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::limits::MAX_PPTX_SLIDES;

pub(super) fn required_slide_order(arguments: &Value, slide_count: usize) -> Result<Vec<usize>> {
    let values = arguments
        .get("slide_order")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slide_order must be an array of positive integers"))?;
    if values.len() != slide_count || values.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "slide_order must contain every current slide position exactly once"
        ));
    }
    let mut seen = HashSet::with_capacity(values.len());
    let mut order = Vec::with_capacity(values.len());
    for value in values {
        let position = value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value >= 1 && *value <= slide_count)
            .ok_or_else(|| anyhow!("slide_order contains an out-of-range slide position"))?;
        if !seen.insert(position) {
            return Err(anyhow!("slide_order must not contain duplicates"));
        }
        order.push(position);
    }
    Ok(order)
}

pub(super) fn required_deleted_slide_positions(
    arguments: &Value,
    slide_count: usize,
) -> Result<Vec<usize>> {
    let values = arguments
        .get("slide_numbers")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slide_numbers must be an array of positive integers"))?;
    if values.is_empty() || values.len() >= slide_count || values.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "slide_numbers must delete at least one slide while leaving at least one slide"
        ));
    }
    parse_unique_slide_numbers(values, slide_count)
}

pub(super) fn selected_slide_positions(
    arguments: &Value,
    slide_count: usize,
) -> Result<Vec<usize>> {
    let Some(value) = arguments.get("slide_numbers") else {
        return Ok((1..=slide_count).collect());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow!("slide_numbers must be an array of positive integers"))?;
    if values.is_empty() || values.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "slide_numbers must contain between 1 and {MAX_PPTX_SLIDES} items"
        ));
    }
    parse_unique_slide_numbers(values, slide_count)
}

fn parse_unique_slide_numbers(values: &[Value], slide_count: usize) -> Result<Vec<usize>> {
    let mut positions = BTreeSet::new();
    for value in values {
        let position = value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value >= 1 && *value <= slide_count)
            .ok_or_else(|| anyhow!("slide_numbers contains an out-of-range slide number"))?;
        if !positions.insert(position) {
            return Err(anyhow!("slide_numbers must not contain duplicates"));
        }
    }
    Ok(positions.into_iter().collect())
}
