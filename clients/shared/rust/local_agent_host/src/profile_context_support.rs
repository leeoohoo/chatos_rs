// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::LocalAgentRun;

pub(crate) const MAXIMUM_SUMMARY_ATTEMPTS: u8 = 8;

pub(crate) fn input_reduction_threshold(run: &LocalAgentRun) -> Result<u64, String> {
    let context = run.model_runtime_snapshot.context_window_tokens;
    let output = u64::from(run.model_runtime_snapshot.maximum_output_tokens);
    let usable = context
        .checked_sub(output)
        .filter(|value| *value > 0)
        .ok_or_else(|| "model descriptor has no usable input context".to_string())?;
    Ok(usable.saturating_mul(4) / 5)
}
