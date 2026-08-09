// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::project_run::ProjectRunCatalog;

use super::RunExecutionInput;

pub(crate) fn resolve_execution(
    catalog: &ProjectRunCatalog,
    input: RunExecutionInput,
) -> Result<(String, String), String> {
    if let Some(target_id) = input
        .target_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(target) = catalog.targets.iter().find(|item| item.id == target_id) {
            return Ok((target.cwd.clone(), target.command.clone()));
        }
        return Err("target_id 不存在".to_string());
    }

    let cwd = input
        .cwd
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "cwd 不能为空".to_string())?
        .to_string();
    let command = input
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "command 不能为空".to_string())?
        .to_string();
    Ok((cwd, command))
}
