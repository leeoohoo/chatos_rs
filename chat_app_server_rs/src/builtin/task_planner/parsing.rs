use serde_json::{Map, Value};

use crate::services::task_manager::{TaskDraft, TaskRequiredContextAssetDraft};
use crate::services::task_service_client::{
    TaskContextAssetRefDto, TaskExecutionResultContractDto,
};

pub(crate) fn parse_task_drafts(args: &Value) -> Result<Vec<TaskDraft>, String> {
    if let Some(items) = args.get("tasks").and_then(Value::as_array) {
        if items.is_empty() && args.get("title").and_then(Value::as_str).is_some() {
            return Ok(vec![task_draft_from_map(
                args.as_object()
                    .ok_or_else(|| "task payload must be an object".to_string())?,
            )?]);
        }

        let mut out = Vec::new();
        for item in items {
            out.push(task_draft_from_value(item)?);
        }
        return Ok(out);
    }

    if args.get("title").and_then(Value::as_str).is_some() {
        return Ok(vec![task_draft_from_map(
            args.as_object()
                .ok_or_else(|| "task payload must be an object".to_string())?,
        )?]);
    }

    Err("tasks or title is required".to_string())
}

pub(crate) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn task_draft_from_value(value: &Value) -> Result<TaskDraft, String> {
    let map = value
        .as_object()
        .ok_or_else(|| "each task must be an object".to_string())?;
    task_draft_from_map(map)
}

fn task_draft_from_map(map: &Map<String, Value>) -> Result<TaskDraft, String> {
    let title = map
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| "task title is required".to_string())?
        .to_string();

    let details = optional_string(map, "details")
        .or_else(|| optional_string(map, "description"))
        .unwrap_or_default();

    let priority = optional_string(map, "priority").unwrap_or_else(|| "medium".to_string());
    let due_at = optional_string(map, "due_at").or_else(|| optional_string(map, "dueAt"));

    let tags = match map.get("tags") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect(),
        Some(Value::String(raw)) => raw
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    };

    Ok(TaskDraft {
        title,
        details,
        priority,
        status: "pending_confirm".to_string(),
        tags,
        due_at,
        required_builtin_capabilities: string_array(
            map.get("required_builtin_capabilities")
                .or_else(|| map.get("requiredBuiltinCapabilities")),
        )
        .unwrap_or_default(),
        required_context_assets: required_context_assets(
            map.get("required_context_assets")
                .or_else(|| map.get("requiredContextAssets")),
        )?,
        planned_builtin_mcp_ids: string_array(map.get("planned_builtin_mcp_ids"))
            .or_else(|| string_array(map.get("plannedBuiltinMcpIds")))
            .unwrap_or_default(),
        planned_context_assets: context_assets(
            map.get("planned_context_assets")
                .or_else(|| map.get("plannedContextAssets")),
        )?,
        execution_result_contract: execution_result_contract(
            map.get("execution_result_contract")
                .or_else(|| map.get("executionResultContract")),
        )?,
    })
}

fn optional_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string())
}

fn string_array(value: Option<&Value>) -> Option<Vec<String>> {
    let Value::Array(items) = value? else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(|item| item.as_str().map(|value| value.trim().to_string()))
            .filter(|value| !value.is_empty())
            .collect(),
    )
}

fn required_context_assets(value: Option<&Value>) -> Result<Vec<TaskRequiredContextAssetDraft>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = value else {
        return Err("required_context_assets must be an array".to_string());
    };

    let mut out = Vec::new();
    for item in items {
        let map = item
            .as_object()
            .ok_or_else(|| "each required_context_assets item must be an object".to_string())?;
        let asset_type = map
            .get("asset_type")
            .or_else(|| map.get("assetType"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "required_context_assets.asset_type is required".to_string())?;
        let asset_ref = map
            .get("asset_ref")
            .or_else(|| map.get("assetRef"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "required_context_assets.asset_ref is required".to_string())?;
        out.push(TaskRequiredContextAssetDraft {
            asset_type: asset_type.to_string(),
            asset_ref: asset_ref.to_string(),
        });
    }
    Ok(out)
}

fn context_assets(value: Option<&Value>) -> Result<Vec<TaskContextAssetRefDto>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = value else {
        return Err("planned_context_assets must be an array".to_string());
    };

    let mut out = Vec::new();
    for item in items {
        let map = item
            .as_object()
            .ok_or_else(|| "each planned_context_assets item must be an object".to_string())?;
        let asset_type = map
            .get("asset_type")
            .or_else(|| map.get("assetType"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "planned_context_assets.asset_type is required".to_string())?;
        let asset_id = map
            .get("asset_id")
            .or_else(|| map.get("assetId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "planned_context_assets.asset_id is required".to_string())?;
        out.push(TaskContextAssetRefDto {
            asset_type: asset_type.to_string(),
            asset_id: asset_id.to_string(),
            display_name: optional_string(map, "display_name")
                .or_else(|| optional_string(map, "displayName")),
            source_type: optional_string(map, "source_type")
                .or_else(|| optional_string(map, "sourceType")),
            source_path: optional_string(map, "source_path")
                .or_else(|| optional_string(map, "sourcePath")),
        });
    }
    Ok(out)
}

fn execution_result_contract(
    value: Option<&Value>,
) -> Result<Option<TaskExecutionResultContractDto>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let map = value
        .as_object()
        .ok_or_else(|| "execution_result_contract must be an object".to_string())?;
    Ok(Some(TaskExecutionResultContractDto {
        result_required: map
            .get("result_required")
            .or_else(|| map.get("resultRequired"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        preferred_format: optional_string(map, "preferred_format")
            .or_else(|| optional_string(map, "preferredFormat")),
    }))
}
