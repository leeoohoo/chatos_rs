// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::memory::external_tool_results;
use chatos_ai_runtime::model_config::normalize_thinking_level;
use chatos_ai_runtime::{
    append_responses_history_items, build_tool_output_items, message_item, user_text_item,
    ModelRuntimeConfig,
};
use chatos_local_agent_protocol::{LocalAgentRunClaim, LocalConversationAttachmentSpec};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(super) fn merge_local_tools(
    mut controlled: Vec<Value>,
    local: &[Value],
    local_prefixes: &[String],
) -> Result<Vec<Value>, String> {
    let mut merged_local = local.to_vec();
    for tool in &mut merged_local {
        let Some(name) = tool_name(tool) else {
            continue;
        };
        let Some(controlled_tool) = controlled
            .iter()
            .find(|candidate| tool_name(candidate) == Some(name))
        else {
            continue;
        };
        merge_task_selection_schema(tool, controlled_tool);
    }
    let local_names = validate_unique_tool_names(&merged_local)?;
    controlled.retain(|tool| {
        tool_name(tool).is_none_or(|name| {
            !local_names.contains(name)
                && !local_prefixes.iter().any(|prefix| name.starts_with(prefix))
        })
    });
    controlled.extend(merged_local);
    Ok(controlled)
}

pub(super) fn main_chat_scoped_tools(mut tools: Vec<Value>, input: &Value) -> Vec<Value> {
    let is_plugin_installer_chat = tools
        .iter()
        .any(|tool| tool_name(tool).is_some_and(|name| name.starts_with("plugin_installer_")));
    if is_plugin_installer_chat {
        // The dedicated installer contact receives a separate immutable capability snapshot.
        // Remove Rust-owned Main Chat helpers as well, so task/project/skill tools are absent
        // from the model request instead of relying on prompt instructions to avoid them.
        tools.retain(|tool| {
            tool_name(tool).is_some_and(|name| name.starts_with("plugin_installer_"))
        });
        return tools;
    }
    if input.pointer("/task_followup_review/pending") == Some(&Value::Bool(true)) {
        // A pending review is the single mutation boundary for the affected
        // Tasks. Do not invite the model to cancel one candidate separately or
        // attempt replacement creation before the atomic review is complete.
        // Read-only status tools remain available for evidence gathering.
        tools.retain(|tool| {
            !matches!(
                tool_name(tool),
                Some("create_task" | "create_tasks_with_prerequisites" | "cancel_task")
            )
        });
    }
    let explicitly_selected_plugin = input
        .pointer("/explicit_capability_selection/kind")
        .and_then(Value::as_str)
        == Some("plugin");
    if explicitly_selected_plugin {
        // The composer selection is already the user's delivery decision. A
        // Requirement Survey is a different explicit surface, not a preflight
        // substitute for dispatching the selected Plugin Task. Any missing
        // task-local detail can be requested by that Task through Ask User.
        tools.retain(|tool| {
            tool_name(tool).is_none_or(|name| {
                !name.starts_with("requirement_survey_") && !name.starts_with("requirement_plan_")
            })
        });
    }
    tools
}

fn merge_task_selection_schema(local: &mut Value, controlled: &Value) {
    let properties_pointer = match tool_name(local) {
        Some("create_task") => "/parameters/properties",
        Some("create_tasks_with_prerequisites") => "/parameters/properties/tasks/items/properties",
        _ => return,
    };
    let Some(controlled_properties) = controlled
        .pointer(properties_pointer)
        .and_then(Value::as_object)
    else {
        return;
    };
    let Some(local_properties) = local
        .pointer_mut(properties_pointer)
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    // The native client resolves the request-scoped Agent capability policy.
    // Keep the durable Rust implementations of the Task tools, but restore the
    // capability and model selection fields from that trusted snapshot as the former
    // Task Runner did. Hard-coding any of these here silently changes product
    // behavior when an administrator edits the Agent binding.
    for key in [
        "enabled_builtin_kinds",
        "external_mcp_config_ids",
        "plugin_hints",
        "default_model_config_id",
        "thinking_level",
    ] {
        if let Some(schema) = controlled_properties.get(key) {
            local_properties.insert(key.to_string(), schema.clone());
        }
    }
}

pub(super) fn validate_unique_tool_names(tools: &[Value]) -> Result<HashSet<&str>, String> {
    let mut names = HashSet::with_capacity(tools.len());
    for tool in tools {
        let name = tool_name(tool)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| "local tool definition is missing a name".to_string())?;
        if !names.insert(name) {
            return Err(format!("local tool definition is duplicated: {name}"));
        }
    }
    Ok(names)
}

pub(super) fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(Value::as_str)
        .or_else(|| tool.pointer("/function/name").and_then(Value::as_str))
}

pub(super) fn completed_async_task_handoff(continuation: Option<&Value>) -> bool {
    continuation
        .and_then(|value| value.get("invocations"))
        .and_then(Value::as_array)
        .is_some_and(|invocations| {
            invocations.iter().any(|invocation| {
                invocation.get("tool_name").and_then(Value::as_str)
                    == Some("wait_for_task_completion")
                    && invocation.get("status").and_then(Value::as_str) == Some("succeeded")
                    && invocation
                        .get("result")
                        .and_then(|result| result.get("accepted"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
        })
}

pub(super) fn completed_task_outcome_report(continuation: Option<&Value>) -> bool {
    continuation
        .and_then(|value| value.get("invocations"))
        .and_then(Value::as_array)
        .is_some_and(|invocations| {
            invocations.iter().any(|invocation| {
                invocation.get("tool_name").and_then(Value::as_str)
                    == Some("task_run_process_report_outcome")
                    && invocation.get("status").and_then(Value::as_str) == Some("succeeded")
                    && invocation
                        .get("result")
                        .and_then(|result| result.get("reported"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
        })
}

pub(super) fn apply_run_thinking_level(
    config: &mut ModelRuntimeConfig,
    input: &Value,
) -> Result<(), String> {
    let Some(settings) = input.get("runtime_settings").and_then(Value::as_object) else {
        return Ok(());
    };
    let enabled = settings
        .get("reasoning_enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| "runtime_settings.reasoning_enabled must be a boolean".to_string())?;
    let requested = if enabled {
        settings
            .get("selected_thinking_level")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "enabled runtime reasoning requires selected_thinking_level".to_string()
            })?
    } else {
        "none"
    };
    config.thinking_level = if let Some(capability) =
        chatos_ai_runtime::model_reasoning::capabilities_for_model(&config.model)
    {
        capability.normalized(Some(requested))
    } else {
        normalize_thinking_level(&config.provider, Some(requested))?
    };
    Ok(())
}

pub(super) fn merge_instructions(
    controlled: Option<String>,
    configured: Option<String>,
) -> Option<String> {
    let controlled = controlled.and_then(non_empty);
    let configured = configured.and_then(non_empty);
    match (controlled, configured) {
        (Some(controlled), Some(configured)) => Some(format!("{controlled}\n\n{configured}")),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

pub(super) fn durable_step_input(
    claim: &LocalAgentRunClaim,
    initial_text_field: &str,
) -> Result<(Vec<Value>, String), String> {
    let checkpoint = claim.run.checkpoint.as_object();
    let Some(response) = checkpoint.and_then(|value| value.get("response")) else {
        let (mut items, mut reason) = initial_step_input(
            &claim.run.input,
            initial_text_field,
            claim.run.model_attempt,
        )?;
        if append_guidance_items(&mut items, claim.run.continuation_input.as_ref())? {
            reason = format!("{reason}_with_guidance");
        }
        return Ok((items, reason));
    };
    let request_items = value_array(response, "request_input_items")?;
    let response_items = value_array(response, "response_output_items")?;
    let internal_items = checkpoint
        .and_then(|value| value.get("continuation"))
        .and_then(|value| value.get("input_items"))
        .map(array_value)
        .transpose()?
        .unwrap_or_default();
    let mut history = append_responses_history_items(
        Value::Array(request_items),
        &response_items,
        &internal_items,
    )
    .as_array()
    .cloned()
    .unwrap_or_default();
    let mut reason = match claim.run.continuation_input.as_ref() {
        Some(value) if value.get("type").and_then(Value::as_str) == Some("tool_results") => {
            history.extend(tool_output_items(claim)?);
            "tool_results".to_string()
        }
        Some(value) if value.get("type").and_then(Value::as_str) == Some("resume") => {
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("resumed");
            let input = value.get("input").cloned().unwrap_or(Value::Null);
            if reason == "ask_user_submitted"
                && input.get("source").and_then(Value::as_str) == Some("ask_user")
            {
                history.push(ask_user_output_item(&response_items, &input)?);
            } else {
                history.push(user_text_item(format!(
                    "Local Agent resumed ({reason}). User input: {}",
                    input
                )));
            }
            "user_resume".to_string()
        }
        Some(value)
            if value.get("type").and_then(Value::as_str) == Some("requirement_survey_resolved") =>
        {
            history.push(requirement_survey_output_item(&response_items, value)?);
            "requirement_survey_resolved".to_string()
        }
        Some(value) if value.get("type").and_then(Value::as_str) == Some("guidance") => {
            "user_guidance".to_string()
        }
        Some(_) => return Err("unsupported durable continuation payload".to_string()),
        None if claim.run.model_attempt > 1 => "model_retry".to_string(),
        None => checkpoint
            .and_then(|value| value.get("continuation"))
            .and_then(|value| value.get("reason"))
            .and_then(Value::as_str)
            .unwrap_or("durable_continuation")
            .to_string(),
    };
    if append_guidance_items(&mut history, claim.run.continuation_input.as_ref())?
        && reason != "user_guidance"
    {
        reason = format!("{reason}_with_guidance");
    }
    Ok((history, reason))
}

fn initial_step_input(
    input: &Value,
    text_field: &str,
    model_attempt: u32,
) -> Result<(Vec<Value>, String), String> {
    if let Some(items) = input.get("input_items") {
        return Ok((array_value(items)?, initial_reason(model_attempt)));
    }
    let text = input
        .get(text_field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let attachments = input
        .get("attachments")
        .map(array_value)
        .transpose()?
        .unwrap_or_default();
    if text.is_none() && attachments.is_empty() {
        return Err(format!("run input requires {text_field} or input_items"));
    }
    Ok((
        vec![local_user_item(text, &attachments)?],
        initial_reason(model_attempt),
    ))
}

fn local_user_item(text: Option<&str>, attachments: &[Value]) -> Result<Value, String> {
    if attachments.is_empty() {
        return Ok(user_text_item(text.unwrap_or_default()));
    }
    let manifest = local_attachment_manifest(attachments)?;
    let mut content = Vec::with_capacity(2);
    if let Some(text) = text {
        content.push(json!({"type": "input_text", "text": text}));
    }
    content.push(json!({
        "type": "input_text",
        "text": format!(
            "Attachment contents have not been loaded yet. Use the run-bound \
             local_attachment_read tool: when delivery=model is advertised, it supplies \
             a complete verified image directly; delivery=content is for bounded text \
             reads. Follow the current tool schema, not an assumed platform capability. \
             By default, answer attached-image questions, translation and short reply \
             drafting directly when image input is available, without creating a Task \
             or using desktop automation merely for attachment access. An explicit \
             user request for a background Task still requires Task orchestration; \
             direct attachment access does not replace that requested workflow. \
             Task delivery=workspace imports the file; rely on visual \
             content only when actual image input is supplied. A manifest, checksum or \
             file path alone is not visual evidence. Treat each \
             authorized_local_ref as an opaque capability. Attachment manifest: {manifest}"
        )
    }));
    Ok(message_item("user", Value::Array(content)))
}

fn append_guidance_items(
    history: &mut Vec<Value>,
    continuation: Option<&Value>,
) -> Result<bool, String> {
    let Some(guidance) = continuation
        .and_then(|value| value.get("guidance"))
        .and_then(Value::as_array)
    else {
        return Ok(false);
    };
    for item in guidance {
        let text = item
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let attachments = item
            .get("attachments")
            .map(array_value)
            .transpose()?
            .unwrap_or_default();
        if text.is_none() && attachments.is_empty() {
            return Err("guidance requires a message or attachments".to_string());
        }
        history.push(local_user_item(text, &attachments)?);
    }
    Ok(!guidance.is_empty())
}

fn local_attachment_manifest(attachments: &[Value]) -> Result<String, String> {
    let records = attachments
        .iter()
        .map(|attachment| {
            let attachment: LocalConversationAttachmentSpec =
                serde_json::from_value(attachment.clone())
                    .map_err(|error| format!("decode local attachment failed: {error}"))?;
            attachment.validate()?;
            Ok(json!({
                "attachment_id": attachment.attachment_id,
                "display_name": attachment.display_name,
                "media_type": attachment.media_type,
                "byte_size": attachment.byte_size,
                "sha256": attachment.sha256,
                "authorized_local_ref": attachment.authorized_local_ref,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    serde_json::to_string(&records)
        .map_err(|error| format!("serialize local attachment manifest failed: {error}"))
}

fn initial_reason(model_attempt: u32) -> String {
    if model_attempt > 1 {
        "model_retry"
    } else {
        "initial_request"
    }
    .to_string()
}

fn value_array(value: &Value, field: &str) -> Result<Vec<Value>, String> {
    value
        .get(field)
        .map(array_value)
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn array_value(value: &Value) -> Result<Vec<Value>, String> {
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "durable input items must be an array".to_string())
}

fn tool_output_items(claim: &LocalAgentRunClaim) -> Result<Vec<Value>, String> {
    Ok(build_tool_output_items(
        external_tool_results(claim)?.as_slice(),
    ))
}

fn ask_user_output_item(response_items: &[Value], input: &Value) -> Result<Value, String> {
    let explicit_call_id = input
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let call_id = explicit_call_id
        .or_else(|| {
            response_items.iter().rev().find_map(|item| {
                let name = item.get("name").and_then(Value::as_str)?;
                name.starts_with("ask_user_")
                    .then(|| item.get("call_id").and_then(Value::as_str))
                    .flatten()
            })
        })
        .ok_or_else(|| "Ask User resume has no matching function call".to_string())?;
    let output = json!({
        "status": "ok",
        "values": input.get("values").cloned().unwrap_or_else(|| json!({})),
        "selection": input.get("selection").cloned().unwrap_or(Value::Null)
    });
    Ok(json!({
        "type": "function_call_output",
        "call_id": call_id,
        "output": output.to_string()
    }))
}

pub(super) fn requirement_survey_output_item(
    response_items: &[Value],
    input: &Value,
) -> Result<Value, String> {
    let call_id = response_items
        .iter()
        .rev()
        .find_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("function_call")
                && item.get("name").and_then(Value::as_str) == Some("requirement_survey_create"))
            .then(|| item.get("call_id").and_then(Value::as_str))
            .flatten()
        })
        .ok_or_else(|| "Requirement Survey resume has no matching function call".to_string())?;
    let answers = input
        .get("answers")
        .filter(|value| value.is_object())
        .ok_or_else(|| "Requirement Survey resume requires submitted answers".to_string())?;
    Ok(json!({
        "type": "function_call_output", "call_id": call_id,
        "output": json!({"status": "resolved", "survey_id": input.get("survey_id"),
            "answers": answers}).to_string()
    }))
}
