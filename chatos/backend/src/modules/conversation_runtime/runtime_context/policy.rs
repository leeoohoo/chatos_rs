// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn merge_optional_system_prompts(
    base: Option<String>,
    appended: Option<String>,
) -> Option<String> {
    match (base, appended) {
        (Some(base), Some(appended)) => Some(format!("{}\n\n{}", base.trim(), appended.trim())),
        (Some(base), None) => Some(base),
        (None, Some(appended)) => Some(appended),
        (None, None) => None,
    }
}
