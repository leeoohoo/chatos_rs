// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{A1Range, RangeReadTarget};

pub(super) fn range_snapshot_id(
    target: &RangeReadTarget,
    range: &A1Range,
    cells: &[Value],
) -> Result<String> {
    let mut hasher = Sha256::new();
    for value in [
        "chatos-excel-range-snapshot-v2",
        std::env::consts::OS,
        target.runtime_instance.as_str(),
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
        range.canonical.as_str(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    let cells = serde_json::to_vec(cells).context("serialize normalized Excel range cells")?;
    hasher.update((cells.len() as u64).to_be_bytes());
    hasher.update(cells);
    Ok(format!("excel_range_{}", hex::encode(hasher.finalize())))
}
