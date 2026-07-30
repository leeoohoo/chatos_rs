// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use tempfile::NamedTempFile;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::delimited_format::{
    parse_delimited, parse_text_cell_reference, serialize_delimited, text_table_rows,
};
use super::format_helpers::{sha256_bytes, sha256_file};
use super::{
    optional_bool, require_extension, required_lowercase_sha256, required_text,
    safe_workspace_path, MAX_ARTIFACT_BYTES,
};

pub(super) fn create_csv(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    create_delimited(arguments, state, request, ".csv", "csv", "CSV", ',')
}

pub(super) fn inspect_tsv(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    inspect_delimited(arguments, state, request, ".tsv", "tsv", "TSV", '\t')
}

pub(super) fn inspect_delimited(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    extension: &str,
    format: &str,
    label: &str,
    delimiter: char,
) -> Result<Value> {
    let requested = required_text(arguments, "path")?;
    let (path, relative) = regular_non_symlink_input_file(state, request, requested, extension)?;
    let (text, source_sha256, bytes) = read_bounded_utf8_delimited(path.as_path(), label)?;
    let document = parse_delimited(text.as_str(), delimiter, label)?;
    let columns = document.rows.iter().map(Vec::len).max().unwrap_or(0);
    let rectangular = document
        .rows
        .first()
        .is_none_or(|first| document.rows.iter().all(|row| row.len() == first.len()));
    Ok(json!({
        "path":relative,
        "format":format,
        "encoding":"utf-8",
        "bytes":bytes,
        "sha256":source_sha256,
        "rows":document.rows.len(),
        "columns":columns,
        "cells":document.rows.iter().map(Vec::len).sum::<usize>(),
        "rectangular":rectangular,
        "line_ending":match document.line_ending {
            Some("\r\n") => "crlf",
            Some("\n") => "lf",
            _ => "none",
        },
        "terminal_record_separator":document.terminal_record_separator,
        "utf8_bom":document.utf8_bom,
    }))
}

pub(super) fn create_tsv(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    create_delimited(arguments, state, request, ".tsv", "tsv", "TSV", '\t')
}

fn create_delimited(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    extension: &str,
    format: &str,
    label: &str,
    delimiter: char,
) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, extension)?;
    let rows = text_table_rows(arguments, "rows", false)?;
    let output = serialize_delimited(rows.as_slice(), delimiter, "\r\n", true, false, label)?;
    let (path, relative) = safe_workspace_path(state, request, target)?;
    write_new_delimited(
        path.as_path(),
        output.as_bytes(),
        optional_bool(arguments, "overwrite"),
        label,
    )?;
    Ok(json!({
        "created":true,
        "path":relative,
        "format":format,
        "encoding":"utf-8",
        "rows":rows.len(),
        "columns":rows.iter().map(Vec::len).max().unwrap_or(0),
        "cells":rows.iter().map(Vec::len).sum::<usize>(),
        "bytes":output.len(),
        "sha256":sha256_file(path.as_path())?,
        "line_ending":"crlf",
        "terminal_record_separator":!rows.is_empty(),
        "formula_injection_protection":true,
    }))
}

pub(super) fn update_tsv_range(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    update_delimited_range(arguments, state, request, ".tsv", "tsv", "TSV", '\t')
}

pub(super) fn update_csv_range(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    update_delimited_range(arguments, state, request, ".csv", "csv", "CSV", ',')
}

fn update_delimited_range(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    extension: &str,
    format: &str,
    label: &str,
    delimiter: char,
) -> Result<Value> {
    let requested = required_text(arguments, "path")?;
    let target = required_text(arguments, "target_path")?;
    require_extension(target, extension)?;
    let expected_sha256 = required_lowercase_sha256(arguments, "expected_sha256")?;
    let (source, source_relative) =
        regular_non_symlink_input_file(state, request, requested, extension)?;
    let (target_path, target_relative) = safe_workspace_path(state, request, target)?;
    ensure_distinct_delimited_paths(source.as_path(), target_path.as_path(), label)?;

    let (source_text, source_sha256, _) = read_bounded_utf8_delimited(source.as_path(), label)?;
    if source_sha256 != expected_sha256 {
        return Err(anyhow!(
            "{label} source does not match expected_sha256; inspect the current file before editing"
        ));
    }
    let mut document = parse_delimited(source_text.as_str(), delimiter, label)?;
    let Some(first_row) = document.rows.first() else {
        return Err(anyhow!(
            "{label} range editing requires a non-empty source table"
        ));
    };
    let columns = first_row.len();
    if columns == 0 || document.rows.iter().any(|row| row.len() != columns) {
        return Err(anyhow!(
            "{label} range editing requires a rectangular source table"
        ));
    }

    let (start_column, start_row) =
        parse_text_cell_reference(required_text(arguments, "start_cell")?, label)?;
    let (end_column, end_row) =
        parse_text_cell_reference(required_text(arguments, "end_cell")?, label)?;
    if end_row < start_row || end_column < start_column {
        return Err(anyhow!(
            "end_cell must be at or below and to the right of start_cell"
        ));
    }
    if end_row > document.rows.len() || end_column > columns {
        return Err(anyhow!(
            "{label} edit range must stay within the existing rectangular table"
        ));
    }
    let replacement = text_table_rows(arguments, "values", true)?;
    let expected_rows = end_row - start_row + 1;
    let expected_columns = end_column - start_column + 1;
    if replacement.len() != expected_rows
        || replacement.iter().any(|row| row.len() != expected_columns)
    {
        return Err(anyhow!(
            "values geometry must exactly match the start_cell/end_cell rectangle"
        ));
    }

    for (row_offset, replacement_row) in replacement.iter().enumerate() {
        for (column_offset, value) in replacement_row.iter().enumerate() {
            document.rows[start_row - 1 + row_offset][start_column - 1 + column_offset] =
                value.clone();
        }
    }
    let line_ending = document.line_ending.unwrap_or("\r\n");
    let output = serialize_delimited(
        document.rows.as_slice(),
        delimiter,
        line_ending,
        document.terminal_record_separator,
        document.utf8_bom,
        label,
    )?;
    write_updated_delimited(
        source.as_path(),
        source_sha256.as_str(),
        target_path.as_path(),
        output.as_bytes(),
        optional_bool(arguments, "overwrite"),
        label,
    )?;
    Ok(json!({
        "updated":true,
        "source_path":source_relative,
        "path":target_relative,
        "format":format,
        "encoding":"utf-8",
        "source_sha256":source_sha256,
        "sha256":sha256_file(target_path.as_path())?,
        "start_cell":required_text(arguments, "start_cell")?.to_ascii_uppercase(),
        "end_cell":required_text(arguments, "end_cell")?.to_ascii_uppercase(),
        "updated_rows":expected_rows,
        "updated_columns":expected_columns,
        "updated_cells":expected_rows.saturating_mul(expected_columns),
        "rows":document.rows.len(),
        "columns":columns,
        "bytes":output.len(),
        "formula_injection_protection":true,
    }))
}

fn regular_non_symlink_input_file(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
    extension: &str,
) -> Result<(PathBuf, String)> {
    require_extension(requested, extension)?;
    let (path, relative) = safe_workspace_path(state, request, requested)?;
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("inspect local artifact {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!(
            "local artifact must be a regular non-symlink file: {relative}"
        ));
    }
    if metadata.len() > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    Ok((path, relative))
}

fn read_bounded_utf8_delimited(path: &Path, label: &str) -> Result<(String, String, usize)> {
    let bytes = fs::read(path).with_context(|| format!("read UTF-8 {label} {}", path.display()))?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let source_sha256 = sha256_bytes(bytes.as_slice());
    if sha256_file(path)? != source_sha256 {
        return Err(anyhow!(
            "{label} source changed while it was being read; inspect the current file again"
        ));
    }
    let bytes_len = bytes.len();
    let text = String::from_utf8(bytes)
        .with_context(|| format!("{label} must be valid UTF-8: {}", path.display()))?;
    Ok((text, source_sha256, bytes_len))
}

fn ensure_distinct_delimited_paths(source: &Path, target: &Path, label: &str) -> Result<()> {
    if source == target {
        return Err(anyhow!(
            "{label} editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    if target.exists() {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect {label} target {}", target.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "{label} target exists and is not a regular non-symlink file"
            ));
        }
        if same_file::is_same_file(source, target)? {
            return Err(anyhow!(
                "{label} editing requires a distinct target_path; source files are never modified in place"
            ));
        }
    }
    Ok(())
}

fn write_new_delimited(target: &Path, content: &[u8], overwrite: bool, label: &str) -> Result<()> {
    if target.exists() {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect {label} target {}", target.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "{label} target exists and is not a regular non-symlink file"
            ));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing {label} without overwrite=true"
            ));
        }
    }
    persist_delimited(target, content, label)
}

fn write_updated_delimited(
    source: &Path,
    source_sha256: &str,
    target: &Path,
    content: &[u8],
    overwrite: bool,
    label: &str,
) -> Result<()> {
    if target.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing {label} without overwrite=true"
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("{label} output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create {label} output directory {}", parent.display()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary {label} in {}", parent.display()))?;
    temporary.write_all(content)?;
    temporary.as_file().sync_all()?;
    if sha256_file(source)? != source_sha256 {
        return Err(anyhow!(
            "{label} source changed while the edit was being prepared; no output was written"
        ));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing {label} {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist {label} {}: {}", target.display(), error.error))?;
    Ok(())
}

fn persist_delimited(target: &Path, content: &[u8], label: &str) -> Result<()> {
    if content.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("{label} exceeds the 100 MiB safety limit"));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("{label} output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create {label} output directory {}", parent.display()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary {label} in {}", parent.display()))?;
    temporary.write_all(content)?;
    temporary.as_file().sync_all()?;
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing {label} {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist {label} {}: {}", target.display(), error.error))?;
    Ok(())
}
