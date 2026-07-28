// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod cancellation;
mod cloning;
mod creation;
mod deletion;
mod updates;

const RETIRED_SELECTED_SKILL_IDS_MESSAGE: &str =
    "selected_skill_ids is retired; select immutable Plugin components through plugin_config.selected_plugins";

fn ensure_legacy_skill_selection_not_written(config: &TaskMcpConfig) -> Result<(), String> {
    if config.selected_skill_ids.is_empty() {
        Ok(())
    } else {
        Err(RETIRED_SELECTED_SKILL_IDS_MESSAGE.to_string())
    }
}

fn prepare_task_mcp_config_update(
    mut requested: TaskMcpConfig,
    existing: &TaskMcpConfig,
) -> Result<TaskMcpConfig, String> {
    ensure_legacy_skill_selection_not_written(&requested)?;
    requested.selected_skill_ids = existing.selected_skill_ids.clone();
    Ok(requested)
}

#[cfg(test)]
mod legacy_skill_selection_tests {
    use super::*;

    #[test]
    fn new_legacy_skill_selection_is_rejected() {
        let config = TaskMcpConfig {
            selected_skill_ids: vec!["internal_skill_documents".to_string()],
            ..TaskMcpConfig::default()
        };

        let error = ensure_legacy_skill_selection_not_written(&config)
            .expect_err("legacy Skill selection must be rejected");

        assert_eq!(error, RETIRED_SELECTED_SKILL_IDS_MESSAGE);
    }

    #[test]
    fn unrelated_updates_preserve_historical_skill_selection() {
        let requested = TaskMcpConfig::default();
        let existing = TaskMcpConfig {
            selected_skill_ids: vec!["legacy-skill".to_string()],
            ..TaskMcpConfig::default()
        };

        let updated = prepare_task_mcp_config_update(requested, &existing)
            .expect("empty legacy selection remains writable");

        assert_eq!(updated.selected_skill_ids, vec!["legacy-skill".to_string()]);
    }
}
