// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::{CreateTaskArgs, CreateTasksArgs};
use crate::application::{
    task_model_policy::resolve_task_model,
    task_tool_source_context::{
        source_attachments, source_conversation_context, source_user_message,
    },
    task_tool_support::*,
};
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunRecord, LocalAgentToolInvocationRecord,
    LocalTaskDependency, LocalTaskSpec,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde_json::json;
use std::collections::HashMap;

pub(in crate::application::task_tools) fn create_single_graph(
    invocation: &LocalAgentToolInvocationRecord,
    parent: &LocalAgentRunRecord,
    args: CreateTaskArgs,
    model: (String, String),
    max_iterations: u32,
) -> Result<CreateTaskGraphCommand, String> {
    let enabled_builtin_kinds =
        validated_builtin_kinds(args.requires_execution, args.enabled_builtin_kinds)?;
    let external_mcp_config_ids = validate_external_mcp_ids(&args.external_mcp_config_ids)?;
    validate_plugin_hints(&args.plugin_hints)?;
    let prompt = task_prompt(
        &args.objective,
        &args.description,
        &args.expected_outputs,
        &args.acceptance_criteria,
        &args.input_payload,
    )?;
    let schedule = contact_async_schedule(args.schedule)?;
    let runtime_settings = task_runtime_settings(args.thinking_level.as_deref())?;
    let graph_id = format!("local-task-graph-{}", invocation.invocation_id);
    let source_context = source_conversation_context(parent);
    let source_user_message = source_user_message(parent);
    let source_attachments = source_attachments(parent)?;
    Ok(CreateTaskGraphCommand {
        graph_id,
        owner_user_id: parent.owner_user_id.clone(),
        source_entity_type: parent.owner_entity_type.clone(),
        source_entity_id: parent.owner_entity_id.clone(),
        tasks: vec![LocalTaskSpec {
            task_id: format!("local-task-{}-1", invocation.invocation_id),
            title: args.title,
            profile_key: "task_execution".to_string(),
            model_config_ref: model.0,
            model_config_revision: model.1,
            capability_policy_revision: parent.capability_policy_revision.clone(),
            input: json!({
                "prompt": prompt,
                "objective": args.objective,
                "expected_outputs": args.expected_outputs,
                "acceptance_criteria": args.acceptance_criteria,
                "acceptance_report_version": 1,
                "final_response_max_chars": args.final_response_max_chars,
                "description": args.description,
                "input_payload": args.input_payload,
                "priority": args.priority,
                "tags": args.tags,
                "owned_paths": args.owned_paths,
                "tool_options": {
                    "requires_execution": args.requires_execution,
                    "enabled_builtin_kinds": enabled_builtin_kinds,
                    "external_mcp_config_ids": external_mcp_config_ids,
                    "plugin_hints": args.plugin_hints,
                    "plugin_release_snapshots": args.plugin_release_snapshots,
                },
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone(),
                "source_user_message": source_user_message,
                "remote_connection_id": source_context.remote_connection_id.clone(),
                "source_run_id": parent.run_id,
                "runtime_settings": runtime_settings,
                "attachments": source_attachments,
                "prerequisite_task_ids": args.prerequisite_task_ids,
                "supersedes_task_ids": args.supersedes_task_ids,
                "followup_message_id": parent.input.pointer("/task_followup_review/review_message_id"),
                "schedule": schedule
            }),
            max_iterations,
        }],
        dependencies: Vec::new(),
    })
}

pub(in crate::application::task_tools) async fn create_batch_graph(
    runtime: &LocalAgentRuntime,
    invocation: &LocalAgentToolInvocationRecord,
    parent: &LocalAgentRunRecord,
    args: CreateTasksArgs,
    max_iterations: u32,
    allowed_model_config_ids: Option<&[String]>,
) -> Result<CreateBatchPlan, String> {
    if args.tasks.len() < 2 || args.tasks.len() > 50 {
        return Err("tasks must contain 2..=50 items".to_string());
    }
    validate_nonempty_text(&args.graph_goal, "graph_goal")?;
    validate_nonempty_text(&args.decomposition_rationale, "decomposition_rationale")?;
    let mut items = args.tasks;
    let mut ref_to_id = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        let client_ref = item.client_ref.trim();
        if client_ref.is_empty() {
            return Err("client_ref cannot be empty".to_string());
        }
        let task_id = format!("local-task-{}-{}", invocation.invocation_id, index + 1);
        if ref_to_id.insert(client_ref.to_string(), task_id).is_some() {
            return Err(format!("client_ref is duplicated: {client_ref}"));
        }
    }
    for item in &items {
        let client_ref = item.client_ref.trim();
        for context_ref in &item.context_refs {
            let context_ref = context_ref.trim();
            if !ref_to_id.contains_key(context_ref) {
                return Err(format!("unknown context_ref: {context_ref}"));
            }
            if context_ref == client_ref {
                return Err(format!("task cannot use itself as context: {context_ref}"));
            }
        }
    }
    if let Some(final_ref) = args.final_delivery_ref.as_deref() {
        if !ref_to_id.contains_key(final_ref.trim()) {
            return Err(format!("unknown final_delivery_ref: {final_ref}"));
        }
    }
    let diagnostics = reduce_client_ref_dependencies(&mut items)?;
    let bindings = items
        .iter()
        .map(|item| CreatedTaskBinding {
            client_ref: item.client_ref.trim().to_string(),
            task_id: ref_to_id
                .get(item.client_ref.trim())
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    let source_context = source_conversation_context(parent);
    let source_user_message = source_user_message(parent);
    let source_attachments = source_attachments(parent)?;
    let mut tasks = Vec::with_capacity(items.len());
    let mut dependencies = Vec::new();
    for item in items {
        let model = resolve_task_model(
            runtime,
            parent,
            item.default_model_config_id.as_deref(),
            allowed_model_config_ids,
        )
        .await?;
        let enabled_builtin_kinds =
            validated_builtin_kinds(item.requires_execution, item.enabled_builtin_kinds)?;
        let external_mcp_config_ids = validate_external_mcp_ids(&item.external_mcp_config_ids)?;
        validate_plugin_hints(&item.plugin_hints)?;
        let input_payload = attach_dependency_context_payload(
            item.input_payload,
            item.client_ref.trim(),
            &item.context_refs,
        );
        let prompt = task_prompt(
            &item.objective,
            &item.description,
            &item.expected_outputs,
            &item.acceptance_criteria,
            &input_payload,
        )?;
        let schedule = contact_async_schedule(item.schedule)?;
        let runtime_settings = task_runtime_settings(item.thinking_level.as_deref())?;
        let task_id = ref_to_id
            .get(item.client_ref.trim())
            .cloned()
            .ok_or_else(|| format!("unknown client_ref: {}", item.client_ref))?;
        for prerequisite_ref in &item.prerequisite_refs {
            let prerequisite_task_id = ref_to_id
                .get(prerequisite_ref.trim())
                .cloned()
                .ok_or_else(|| format!("unknown prerequisite_ref: {prerequisite_ref}"))?;
            dependencies.push(LocalTaskDependency {
                task_id: task_id.clone(),
                prerequisite_task_id,
            });
        }
        tasks.push(LocalTaskSpec {
            task_id,
            title: item.title,
            profile_key: "task_execution".to_string(),
            model_config_ref: model.0,
            model_config_revision: model.1,
            capability_policy_revision: parent.capability_policy_revision.clone(),
            input: json!({
                "prompt": prompt,
                "objective": item.objective,
                "expected_outputs": item.expected_outputs,
                "acceptance_criteria": item.acceptance_criteria,
                "acceptance_report_version": 1,
                "final_response_max_chars": item.final_response_max_chars,
                "graph_goal": args.graph_goal,
                "decomposition_rationale": args.decomposition_rationale,
                "graph_notification_policy": args.notification_policy.as_str(),
                "notification_milestone": item.notification_milestone,
                "final_delivery": args.final_delivery_ref.as_deref().is_some_and(|value| value.trim() == item.client_ref.trim()),
                "description": item.description,
                "input_payload": input_payload,
                "client_ref": item.client_ref,
                "priority": item.priority,
                "tags": item.tags,
                "owned_paths": item.owned_paths,
                "tool_options": {
                    "requires_execution": item.requires_execution,
                    "enabled_builtin_kinds": enabled_builtin_kinds,
                    "external_mcp_config_ids": external_mcp_config_ids,
                    "plugin_hints": item.plugin_hints,
                    "plugin_release_snapshots": item.plugin_release_snapshots,
                },
                "source_conversation_id": source_context.conversation_id.clone(),
                "source_turn_id": source_context.turn_id.clone(),
                "source_user_message": source_user_message.clone(),
                "remote_connection_id": source_context.remote_connection_id.clone(),
                "source_run_id": parent.run_id,
                "runtime_settings": runtime_settings,
                "attachments": source_attachments.clone(),
                "prerequisite_task_ids": item.prerequisite_task_ids,
                "supersedes_task_ids": args.supersedes_task_ids,
                "followup_message_id": parent.input.pointer("/task_followup_review/review_message_id"),
                "schedule": schedule
            }),
            max_iterations,
        });
    }
    Ok(CreateBatchPlan {
        command: CreateTaskGraphCommand {
            graph_id: format!("local-task-graph-{}", invocation.invocation_id),
            owner_user_id: parent.owner_user_id.clone(),
            source_entity_type: parent.owner_entity_type.clone(),
            source_entity_id: parent.owner_entity_id.clone(),
            tasks,
            dependencies,
        },
        bindings,
        diagnostics,
    })
}

pub(in crate::application::task_tools) struct CreateBatchPlan {
    pub(in crate::application::task_tools) command: CreateTaskGraphCommand,
    pub(in crate::application::task_tools) bindings: Vec<CreatedTaskBinding>,
    pub(in crate::application::task_tools) diagnostics: DependencyReduction,
}
