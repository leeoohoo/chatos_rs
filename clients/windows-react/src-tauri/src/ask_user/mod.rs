mod models;
mod projection;
#[cfg(test)]
mod tests;

use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CancelTaskCommand, GetConversationCommand, GetRunCommand,
    HostCommand, HostResult, ListEventsCommand, ListRunsCommand, LocalAgentEventPayloadMode,
    LocalAgentRunListScope, LocalAgentRunRecord, LocalAgentRunStatus,
    ResumeConversationTurnCommand, ResumeRunCommand,
};
use models::{
    AskUserMutationInput, AskUserPrompt, AskUserPromptStatus, AskUserSelectionInput,
    AskUserSubmissionInput,
};
use projection::{prompt, run_context, sensitive_key, AskUserRunContext};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;
use uuid::Uuid;

use crate::host_lifecycle::HostManager;

const PROMPT_LIMIT: u32 = 100;

#[tauri::command]
pub async fn load_ask_user_prompts(
    manager: State<'_, HostManager>,
    owner_user_id: String,
    conversation_id: String,
) -> Result<Vec<AskUserPrompt>, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let conversation_id = required("conversation_id", conversation_id)?;
    let result = manager
        .execute(HostCommand::ListRuns(ListRunsCommand {
            owner_user_id: owner_user_id.clone(),
            scope: LocalAgentRunListScope::Active,
            status: Some(LocalAgentRunStatus::WaitingUser),
            updated_after_unix_ms: None,
            before_updated_at_unix_ms: None,
            before_run_id: None,
            limit: PROMPT_LIMIT,
        }))
        .await?;
    let HostResult::Runs { page } = result else {
        return Err("Local Agent Host 返回了错误的等待回复列表。".into());
    };
    let mut prompts = Vec::new();
    for run in page.runs {
        if run.owner_user_id != owner_user_id || run.status != LocalAgentRunStatus::WaitingUser {
            continue;
        }
        let Some(context) = run_context(&run.profile_key, &run.input) else {
            continue;
        };
        if context.conversation_id != conversation_id {
            continue;
        }
        if let Some(value) = latest_prompt(
            &manager,
            &owner_user_id,
            &run.run_id,
            run.updated_at_unix_ms,
            &context,
        )
        .await?
        {
            prompts.push(value);
        }
    }
    prompts.sort_by(|left, right| {
        (left.created_at_unix_ms, left.id.as_str())
            .cmp(&(right.created_at_unix_ms, right.id.as_str()))
    });
    Ok(prompts)
}

#[tauri::command]
pub async fn submit_ask_user_prompt(
    manager: State<'_, HostManager>,
    input: AskUserMutationInput,
) -> Result<AskUserPrompt, String> {
    let input = validate_mutation(input, true)?;
    let submission = input
        .submission
        .as_ref()
        .ok_or_else(|| "submission is required".to_owned())?;
    let resolved = resolve(&manager, &input).await?;
    validate_submission(submission, &resolved.prompt)?;
    let continuation_input = submission_input(submission, &resolved.prompt);
    if resolved.run.profile_key == "main_chat" {
        let conversation_version = conversation_version(
            &manager,
            &resolved.run.owner_user_id,
            &resolved.context.conversation_id,
        )
        .await?;
        let result = manager
            .execute(HostCommand::ResumeConversationTurn(
                ResumeConversationTurnCommand {
                    owner_user_id: resolved.run.owner_user_id.clone(),
                    conversation_id: resolved.context.conversation_id.clone(),
                    expected_conversation_version: conversation_version,
                    turn_id: resolved.context.turn_id.clone(),
                    expected_run_version: resolved.run.version,
                    expected_run_status: LocalAgentRunStatus::WaitingUser,
                    message_id: format!("local-ask-response-{}", Uuid::new_v4()),
                    message: submission_message(submission),
                    message_metadata: continuation_input,
                    attachments: Vec::new(),
                    reason: "ask_user_submitted".into(),
                },
            ))
            .await?;
        if !matches!(result, HostResult::ConversationTurnUpdated { .. }) {
            return Err("Local Agent Host 返回了错误的会话恢复响应。".into());
        }
    } else {
        let result = manager
            .execute(HostCommand::ResumeRun(ResumeRunCommand {
                owner_user_id: resolved.run.owner_user_id.clone(),
                run_id: resolved.run.run_id.clone(),
                expected_version: resolved.run.version,
                expected_status: LocalAgentRunStatus::WaitingUser,
                reason: "ask_user_submitted".into(),
                input: continuation_input,
            }))
            .await?;
        if !matches!(result, HostResult::Run { .. }) {
            return Err("Local Agent Host 返回了错误的 Run 恢复响应。".into());
        }
    }
    Ok(resolved
        .prompt
        .with_status(AskUserPromptStatus::Ok, now_unix_ms()))
}

#[tauri::command]
pub async fn cancel_ask_user_prompt(
    manager: State<'_, HostManager>,
    input: AskUserMutationInput,
) -> Result<AskUserPrompt, String> {
    let input = validate_mutation(input, false)?;
    let resolved = resolve(&manager, &input).await?;
    if !resolved.prompt.allows_cancel {
        return Err("这个交互请求不允许取消。".into());
    }
    if resolved.run.profile_key == "main_chat" {
        let conversation_version = conversation_version(
            &manager,
            &resolved.run.owner_user_id,
            &resolved.context.conversation_id,
        )
        .await?;
        let result = manager
            .execute(HostCommand::CancelConversationTurn(
                CancelConversationTurnCommand {
                    owner_user_id: resolved.run.owner_user_id.clone(),
                    conversation_id: resolved.context.conversation_id.clone(),
                    expected_conversation_version: conversation_version,
                    turn_id: resolved.context.turn_id.clone(),
                    expected_run_version: Some(resolved.run.version),
                    reason: "user_cancelled".into(),
                },
            ))
            .await?;
        if !matches!(result, HostResult::ConversationTurnUpdated { .. }) {
            return Err("Local Agent Host 返回了错误的会话取消响应。".into());
        }
    } else {
        if resolved.run.owner_entity_type != "task" {
            return Err("这个 Run 不能通过 Ask User 取消。".into());
        }
        let result = manager
            .execute(HostCommand::CancelTask(CancelTaskCommand {
                owner_user_id: resolved.run.owner_user_id.clone(),
                task_id: resolved.run.owner_entity_id.clone(),
                expected_version: None,
                reason: "user_cancelled".into(),
                replacement_task_ids: Vec::new(),
            }))
            .await?;
        if !matches!(result, HostResult::TaskGraph { .. }) {
            return Err("Local Agent Host 返回了错误的任务取消响应。".into());
        }
    }
    Ok(resolved
        .prompt
        .with_status(AskUserPromptStatus::Canceled, now_unix_ms()))
}

struct ResolvedPrompt {
    run: LocalAgentRunRecord,
    context: AskUserRunContext,
    prompt: AskUserPrompt,
}

async fn resolve(
    manager: &HostManager,
    input: &AskUserMutationInput,
) -> Result<ResolvedPrompt, String> {
    let run_id = input
        .prompt_id
        .strip_prefix("local-ask:")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "交互请求不存在或已经结束。".to_owned())?;
    let result = manager
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: input.owner_user_id.clone(),
            run_id: run_id.to_owned(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("Local Agent Host 返回了错误的 Run 响应。".into());
    };
    let context = run_context(&run.profile_key, &run.input)
        .filter(|context| context.conversation_id == input.conversation_id);
    if run.owner_user_id != input.owner_user_id
        || run.run_id != run_id
        || run.status != LocalAgentRunStatus::WaitingUser
        || context.is_none()
    {
        return Err("交互请求不存在或已经结束。".into());
    }
    let context = context.expect("context was checked above");
    let prompt = latest_prompt(
        manager,
        &run.owner_user_id,
        &run.run_id,
        run.updated_at_unix_ms,
        &context,
    )
    .await?
    .ok_or_else(|| "交互请求不存在或已经结束。".to_owned())?;
    Ok(ResolvedPrompt {
        run,
        context,
        prompt,
    })
}

async fn latest_prompt(
    manager: &HostManager,
    owner_user_id: &str,
    run_id: &str,
    updated_at_unix_ms: i64,
    context: &AskUserRunContext,
) -> Result<Option<AskUserPrompt>, String> {
    let result = manager
        .execute(HostCommand::ListEvents(ListEventsCommand {
            owner_user_id: owner_user_id.to_owned(),
            after_cursor: 0,
            limit: 1,
            run_id: Some(run_id.to_owned()),
            event_type: Some("user_input_requested".into()),
            newest_first: true,
            payload_mode: LocalAgentEventPayloadMode::Full,
        }))
        .await?;
    let HostResult::Events { events, .. } = result else {
        return Err("Local Agent Host 返回了错误的交互请求响应。".into());
    };
    Ok(events
        .first()
        .and_then(|event| prompt(run_id, updated_at_unix_ms, context, event)))
}

async fn conversation_version(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<u64, String> {
    let result = manager
        .execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Local Agent Host 返回了错误的会话响应。".into());
    };
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
    {
        return Err("Local Agent Host 返回了不匹配的会话。".into());
    }
    Ok(conversation.conversation.version)
}

fn validate_submission(
    submission: &AskUserSubmissionInput,
    prompt: &AskUserPrompt,
) -> Result<(), String> {
    let known_keys = prompt
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect::<HashSet<_>>();
    for (key, value) in &submission.values {
        if !known_keys.contains(key.as_str()) {
            return Err("回复包含未知字段。".into());
        }
        let secret = prompt
            .fields
            .iter()
            .any(|field| field.key == *key && field.is_secret);
        if (secret || sensitive_key(key)) && !value.is_empty() {
            return Err("Secret 回答不能通过 Local Agent IPC 发送或写入数据库。".into());
        }
    }
    for field in prompt.fields.iter().filter(|field| field.is_required) {
        if submission
            .values
            .get(&field.key)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(format!("“{}”不能为空。", field.label));
        }
    }
    validate_selection(submission.selection.as_ref(), prompt.choice.as_ref())
}

fn validate_selection(
    selection: Option<&AskUserSelectionInput>,
    choice: Option<&models::AskUserChoice>,
) -> Result<(), String> {
    let Some(choice) = choice else {
        return if selection.is_none() {
            Ok(())
        } else {
            Err("这个交互请求不接受选项。".into())
        };
    };
    let selected = match (choice.allows_multiple, selection) {
        (true, Some(AskUserSelectionInput::Multiple(values))) => values.clone(),
        (true, None) => Vec::new(),
        (false, Some(AskUserSelectionInput::Single(value))) if !value.is_empty() => {
            vec![value.clone()]
        }
        (false, Some(AskUserSelectionInput::Single(_))) | (false, None) => Vec::new(),
        _ => return Err("选项类型与交互请求不匹配。".into()),
    };
    let unique = selected.iter().collect::<HashSet<_>>();
    if unique.len() != selected.len()
        || selected
            .iter()
            .any(|value| !choice.options.iter().any(|option| option.value == *value))
    {
        return Err("回复包含无效选项。".into());
    }
    if selected.len() < choice.minimum_selection_count
        || selected.len() > choice.maximum_selection_count
    {
        return Err(format!(
            "请选择 {}–{} 项。",
            choice.minimum_selection_count, choice.maximum_selection_count
        ));
    }
    Ok(())
}

fn submission_input(submission: &AskUserSubmissionInput, prompt: &AskUserPrompt) -> Value {
    let selection = match &submission.selection {
        Some(AskUserSelectionInput::Single(value)) => Value::String(value.clone()),
        Some(AskUserSelectionInput::Multiple(values)) => json!(values),
        None => Value::Null,
    };
    json!({
        "source": "ask_user",
        "prompt_id": prompt.id,
        "tool_call_id": prompt.tool_call_id,
        "values": submission.values,
        "selection": selection,
    })
}

fn submission_message(submission: &AskUserSubmissionInput) -> String {
    let mut lines = submission
        .values
        .iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>();
    match &submission.selection {
        Some(AskUserSelectionInput::Single(value)) if !value.is_empty() => {
            lines.push(value.clone())
        }
        Some(AskUserSelectionInput::Multiple(values)) if !values.is_empty() => {
            lines.push(values.join(", "))
        }
        _ => {}
    }
    if lines.is_empty() {
        "已提交回复".into()
    } else {
        lines.join("\n")
    }
}

fn validate_mutation(
    mut input: AskUserMutationInput,
    requires_submission: bool,
) -> Result<AskUserMutationInput, String> {
    input.owner_user_id = required("owner_user_id", input.owner_user_id)?;
    input.conversation_id = required("conversation_id", input.conversation_id)?;
    input.prompt_id = required("prompt_id", input.prompt_id)?;
    if requires_submission != input.submission.is_some() {
        return Err(if requires_submission {
            "submission is required"
        } else {
            "submission is not allowed"
        }
        .into());
    }
    Ok(input)
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
