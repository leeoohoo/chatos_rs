// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::MAX_PDF_PAGES;

pub(super) fn required_pdf_paths(
    arguments: &Value,
    field: &str,
    min_items: usize,
    max_items: usize,
) -> Result<Vec<String>> {
    let items = arguments
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if items.len() < min_items || items.len() > max_items {
        return Err(anyhow!(
            "{field} must contain between {min_items} and {max_items} PDF paths"
        ));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("{field} must contain only non-empty strings"))
        })
        .collect()
}

pub(super) fn required_page_numbers(
    arguments: &Value,
    field: &str,
    page_count: usize,
) -> Result<Vec<u32>> {
    optional_page_numbers(arguments, field, page_count)?
        .ok_or_else(|| anyhow!("{field} is required"))
}

pub(super) fn required_page_sequence(
    arguments: &Value,
    field: &str,
    page_count: usize,
) -> Result<Vec<u32>> {
    let items = arguments
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if items.is_empty() || items.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "{field} must contain between 1 and {MAX_PDF_PAGES} page numbers"
        ));
    }
    let mut seen = HashSet::with_capacity(items.len());
    let mut pages = Vec::with_capacity(items.len());
    for item in items {
        let page = item
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value >= 1 && *value as usize <= page_count)
            .ok_or_else(|| anyhow!("{field} contains a page outside 1..={page_count}"))?;
        if !seen.insert(page) {
            return Err(anyhow!("{field} must contain unique page numbers"));
        }
        pages.push(page);
    }
    Ok(pages)
}

pub(super) fn optional_page_numbers(
    arguments: &Value,
    field: &str,
    page_count: usize,
) -> Result<Option<Vec<u32>>> {
    let Some(value) = arguments.get(field) else {
        return Ok(None);
    };
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if items.is_empty() || items.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "{field} must contain between 1 and {MAX_PDF_PAGES} page numbers"
        ));
    }
    let mut pages = Vec::with_capacity(items.len());
    for item in items {
        let page = item
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value >= 1 && *value as usize <= page_count)
            .ok_or_else(|| anyhow!("{field} contains a page outside 1..={page_count}"))?;
        if pages.last().is_some_and(|previous| *previous >= page) {
            return Err(anyhow!(
                "{field} must contain unique page numbers in ascending order"
            ));
        }
        pages.push(page);
    }
    Ok(Some(pages))
}
