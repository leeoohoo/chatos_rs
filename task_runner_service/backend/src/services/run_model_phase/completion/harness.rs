// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::TaskRunReport;

use crate::services::harness_run_git::HarnessRunOutputReport;

pub(super) fn fail_report_when_promotion_failed(
    report: &mut TaskRunReport,
    harness_output: Option<&HarnessRunOutputReport>,
) {
    let Some(harness) = harness_output
        .filter(|harness| matches!(harness.status.as_str(), "failed" | "merge_conflict"))
    else {
        return;
    };
    let harness_error = harness
        .message
        .as_deref()
        .unwrap_or("Harness output promotion failed");
    report.status = chatos_ai_runtime::AiTurnStatus::Failed;
    report.error = Some(match report.error.take() {
        Some(error) => format!("{error}; Harness output promotion failed: {harness_error}"),
        None => format!("Harness output promotion failed: {harness_error}"),
    });
}
