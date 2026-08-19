// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::RollupSettings;

const ROLLUP_COMPAT_TOKEN_LIMIT: i64 = 6000;
const ROLLUP_COMPAT_TARGET_TOKENS: i64 = 700;

#[allow(dead_code)]
pub fn default_rollup_settings() -> RollupSettings {
    RollupSettings {
        token_limit: ROLLUP_COMPAT_TOKEN_LIMIT,
        target_summary_tokens: ROLLUP_COMPAT_TARGET_TOKENS,
        count_limit: 0,
        keep_level0_count: 5,
        max_level: 4,
        cloud_owner_entity_id: None,
        cloud_source_id: None,
        cloud_thread_id: None,
    }
}
