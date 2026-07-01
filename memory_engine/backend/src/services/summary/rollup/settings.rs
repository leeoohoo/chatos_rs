// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::{RollupSettings, DEFAULT_ROLLUP_TARGET_TOKENS, DEFAULT_ROLLUP_TOKEN_LIMIT};

#[allow(dead_code)]
pub fn default_rollup_settings() -> RollupSettings {
    RollupSettings {
        summary_prompt: None,
        token_limit: DEFAULT_ROLLUP_TOKEN_LIMIT,
        target_summary_tokens: DEFAULT_ROLLUP_TARGET_TOKENS,
        count_limit: 0,
        keep_level0_count: 5,
        max_level: 4,
    }
}
