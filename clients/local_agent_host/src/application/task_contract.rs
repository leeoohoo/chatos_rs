// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_tool_source_context::source_user_message,
    task_tool_support::validate_nonempty_text,
    task_tools::{
        LocalTaskToolExecutor, TaskPluginHint, TaskRequirement, TaskVerificationContract,
    },
};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalConversationMessageRole};
use serde_json::Value;
use std::collections::HashSet;

pub(super) struct TaskContractView<'a> {
    pub(super) requirement_refs: Vec<&'a str>,
    pub(super) expected_outputs: &'a [String],
    pub(super) acceptance_criteria: &'a [String],
    pub(super) requires_execution: bool,
    pub(super) enabled_builtin_kinds: &'a [String],
    pub(super) plugin_hints: &'a [TaskPluginHint],
    pub(super) owned_paths: &'a [String],
    pub(super) verification_contract: Option<&'a TaskVerificationContract>,
}

pub(super) async fn validate_task_contracts(
    executor: &LocalTaskToolExecutor,
    parent: &LocalAgentRunRecord,
    conversation_id: &str,
    requirements: &mut [TaskRequirement],
    tasks: &[TaskContractView<'_>],
) -> Result<(), String> {
    if requirements.is_empty() || requirements.len() > 50 {
        return Err("requirements must contain 1..=50 items".to_string());
    }
    let current_message_id = parent
        .input
        .get("message_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let current_message = current_message_id
        .as_deref()
        .zip(source_user_message(parent));
    let confirmed_plan_markdown = parent
        .input
        .get("requirement_plan")
        .filter(|plan| {
            plan.get("confirmed_at_unix_ms")
                .is_some_and(|confirmed| !confirmed.is_null())
        })
        .and_then(|plan| plan.get("markdown"))
        .and_then(Value::as_str);
    let mut source_messages = Vec::new();
    if let Some(conversation) = executor
        .runtime
        .conversation_for_host_worker(&parent.owner_user_id, conversation_id)
        .await
        .map_err(|error| error.to_string())?
    {
        for message in conversation
            .messages
            .into_iter()
            .filter(|message| message.role == LocalConversationMessageRole::User)
        {
            if let Some(text) = conversation_message_text(&message.content) {
                source_messages.push((message.message_id, text));
            }
        }
    }
    if let Some((message_id, message)) = current_message {
        if !source_messages.iter().any(|(id, _)| id == message_id) {
            source_messages.push((message_id.to_string(), message));
        }
    }

    let mut ids = HashSet::new();
    let mut requires_change = HashSet::new();
    for requirement in requirements.iter_mut() {
        let id = requirement.requirement_id.trim();
        validate_nonempty_text(id, "requirement_id")?;
        if !ids.insert(id.to_string()) {
            return Err(format!("requirement_id is duplicated: {id}"));
        }
        let quote = requirement.source_quote.trim();
        if quote.is_empty() {
            return Err(format!(
                "requirement {id} source_quote must be an exact excerpt of an authoritative user message"
            ));
        }
        requirement.source_message_id = resolve_source_message_id(
            id,
            quote,
            current_message_id.as_deref(),
            confirmed_plan_markdown,
            &source_messages,
        )?;
        let interpreted = requirement.interpreted_requirement.trim();
        if interpreted.chars().count() < 4
            || matches!(
                interpreted.to_ascii_lowercase().as_str(),
                "handle it" | "do it" | "处理一下" | "按上文修改"
            )
        {
            return Err(format!(
                "requirement {id} interpreted_requirement must state an observable result"
            ));
        }
        if requirement.requires_change {
            requires_change.insert(id.to_string());
        }
    }

    let mut referenced = HashSet::new();
    let mut change_owners = HashSet::new();
    let explicit_plugin = (parent
        .input
        .pointer("/explicit_capability_selection/kind")
        .and_then(Value::as_str)
        == Some("plugin"))
    .then(|| {
        Some((
            parent
                .input
                .pointer("/explicit_capability_selection/plugin_key")
                .and_then(Value::as_str)?,
            parent
                .input
                .pointer("/explicit_capability_selection/component_key")
                .and_then(Value::as_str)?,
        ))
    })
    .flatten();
    for task in tasks {
        if task.requirement_refs.is_empty() {
            return Err("every Task must reference at least one requirement_id".to_string());
        }
        let mut task_refs = HashSet::new();
        for reference in &task.requirement_refs {
            let reference = reference.trim();
            if !ids.contains(reference) {
                return Err(format!("unknown requirement_ref: {reference}"));
            }
            if !task_refs.insert(reference.to_string()) {
                return Err(format!(
                    "requirement_ref is duplicated in one Task: {reference}"
                ));
            }
            referenced.insert(reference.to_string());
            if task_can_change(task, explicit_plugin) {
                change_owners.insert(reference.to_string());
            }
        }
        for (field, entries) in [
            ("expected_outputs", task.expected_outputs),
            ("acceptance_criteria", task.acceptance_criteria),
        ] {
            for entry in entries {
                if !task_refs
                    .iter()
                    .any(|id| entry.contains(&format!("[{id}]")))
                {
                    return Err(format!(
                        "every {field} entry must include a bound requirement marker such as [R1]"
                    ));
                }
            }
        }
        validate_verification_contract(task)?;
    }
    if let Some(unreferenced) = ids.difference(&referenced).next() {
        return Err(format!(
            "requirement {unreferenced} is not assigned to any Task"
        ));
    }
    if let Some(unowned) = requires_change.difference(&change_owners).next() {
        return Err(format!(
            "change requirement {unowned} must be assigned to at least one execution Task with write capability; a read-only verification Task cannot be its only owner"
        ));
    }
    Ok(())
}

fn resolve_source_message_id(
    requirement_id: &str,
    quote: &str,
    current_message_id: Option<&str>,
    confirmed_plan_markdown: Option<&str>,
    source_messages: &[(String, String)],
) -> Result<String, String> {
    if let Some(message_id) = current_message_id.filter(|message_id| {
        source_messages
            .iter()
            .any(|(id, message)| id == *message_id && message.contains(quote))
    }) {
        return Ok(message_id.to_string());
    }
    if confirmed_plan_markdown.is_some_and(|markdown| markdown.contains(quote)) {
        return current_message_id.map(str::to_string).ok_or_else(|| {
            format!(
                "requirement {requirement_id} cannot bind its confirmed-plan source without a current user message"
            )
        });
    }
    let mut matches = source_messages
        .iter()
        .rev()
        .filter(|(_, message)| message.contains(quote));
    let Some((message_id, message)) = matches.next() else {
        return Err(format!(
            "requirement {requirement_id} source_quote is not an exact excerpt of an authoritative user message"
        ));
    };
    if matches.any(|(_, candidate)| candidate != message) {
        return Err(format!(
            "requirement {requirement_id} source_quote matches multiple authoritative user messages; provide a longer unique excerpt"
        ));
    }
    Ok(message_id.clone())
}

fn conversation_message_text(content: &Value) -> Option<String> {
    content
        .as_str()
        .or_else(|| content.get("text").and_then(Value::as_str))
        .or_else(|| content.get("content").and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn task_can_change(task: &TaskContractView<'_>, explicit_plugin: Option<(&str, &str)>) -> bool {
    task.requires_execution
        && (task.enabled_builtin_kinds.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "CodeMaintainerWrite"
                    | "RemoteConnectionController"
                    | "RequirementSurveyWrite"
                    | "Notepad"
            )
        }) || explicit_plugin.is_some_and(|(plugin_key, component_key)| {
            task.plugin_hints
                .iter()
                .any(|hint| hint.plugin_key == plugin_key && hint.component_key == component_key)
        }) || (!task.plugin_hints.is_empty() && task.owned_paths.is_empty()))
}

fn validate_verification_contract(task: &TaskContractView<'_>) -> Result<(), String> {
    let Some(contract) = task.verification_contract else {
        return Ok(());
    };
    if !task.requires_execution {
        return Err("verification_contract requires requires_execution=true".to_string());
    }
    validate_nonempty_text(&contract.subject_identity, "subject_identity")?;
    validate_nonempty_text(&contract.workflow_entry, "workflow_entry")?;
    if contract.checks.is_empty() || contract.checks.len() > 50 {
        return Err("verification_contract.checks must contain 1..=50 items".to_string());
    }
    let mut ids = HashSet::new();
    for check in &contract.checks {
        let id = check.check_id.trim();
        validate_nonempty_text(id, "check_id")?;
        if !ids.insert(id.to_string()) {
            return Err(format!("verification check_id is duplicated: {id}"));
        }
    }
    for check in &contract.checks {
        let mut dependencies = HashSet::new();
        let check_id = check.check_id.trim();
        for dependency in &check.depends_on {
            let dependency = dependency.trim();
            if dependency == check_id {
                return Err(format!(
                    "verification check cannot depend on itself: {dependency}"
                ));
            }
            if !ids.contains(dependency) {
                return Err(format!("unknown verification dependency: {dependency}"));
            }
            if !dependencies.insert(dependency) {
                return Err(format!(
                    "verification dependency is duplicated for {}: {dependency}",
                    check_id
                ));
            }
        }
    }
    let mut resolved = HashSet::new();
    loop {
        let before = resolved.len();
        for check in &contract.checks {
            if check
                .depends_on
                .iter()
                .all(|dependency| resolved.contains(dependency.trim()))
            {
                resolved.insert(check.check_id.trim());
            }
        }
        if resolved.len() == contract.checks.len() {
            return Ok(());
        }
        if resolved.len() == before {
            return Err("verification_contract.checks contains a dependency cycle".to_string());
        }
    }
}

#[cfg(test)]
mod source_binding_tests {
    use super::*;

    #[test]
    fn source_message_identity_is_resolved_without_model_owned_ids() {
        let messages = vec![
            (
                "current".to_string(),
                "continue the shared work".to_string(),
            ),
            ("older-1".to_string(), "inspect the project".to_string()),
            ("older-2".to_string(), "continue the old work".to_string()),
        ];

        assert_eq!(
            resolve_source_message_id("R1", "continue", Some("current"), None, &messages).unwrap(),
            "current"
        );
        assert_eq!(
            resolve_source_message_id(
                "R1",
                "inspect the project",
                Some("current"),
                None,
                &messages,
            )
            .unwrap(),
            "older-1"
        );
        assert!(
            resolve_source_message_id("R1", "work", None, None, &messages)
                .unwrap_err()
                .contains("multiple")
        );
        assert!(
            resolve_source_message_id("R1", "missing", Some("current"), None, &messages)
                .unwrap_err()
                .contains("not an exact excerpt")
        );
    }

    #[test]
    fn repeated_identical_source_messages_bind_to_the_latest_one() {
        let messages = vec![
            ("older".to_string(), "download this video".to_string()),
            ("latest".to_string(), "download this video".to_string()),
        ];

        assert_eq!(
            resolve_source_message_id("R1", "download this video", None, None, &messages).unwrap(),
            "latest"
        );
    }
}
