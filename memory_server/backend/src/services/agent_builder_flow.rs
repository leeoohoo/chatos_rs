use axum::http::StatusCode;
use reqwest::Client;
use serde_json::{json, Value};

use crate::models::MemoryAgent;
use crate::repositories::{agents as agents_repo, skills as skills_repo};

use super::{
    bad_gateway_error,
    create_support::build_create_agent_request,
    internal_error,
    request_support::request_chat_completion,
    stream_support::{extract_message_text, parse_tool_calls},
    support::{
        build_agent_builder_tools, build_plain_system_prompt, build_plain_user_prompt,
        build_tool_loop_system_prompt, build_tool_loop_user_prompt, is_tooling_unsupported,
        parse_json_candidate,
    },
    tool_support::execute_tool_call,
    ModelRuntime, ToolContext, ToolLoopOutcome,
};

pub(super) async fn run_agent_builder(
    http: &Client,
    runtime: &ModelRuntime,
    context: &mut ToolContext<'_>,
) -> Result<ToolLoopOutcome, (StatusCode, String)> {
    if runtime.supports_responses {
        return run_plain_json_fallback(http, runtime, context).await;
    }

    match run_tool_loop(http, runtime, context).await {
        Ok(outcome) => Ok(outcome),
        Err((status, detail))
            if status == StatusCode::BAD_GATEWAY && is_tooling_unsupported(detail.as_str()) =>
        {
            run_plain_json_fallback(http, runtime, context).await
        }
        Err(err) => Err(err),
    }
}

async fn run_tool_loop(
    http: &Client,
    runtime: &ModelRuntime,
    context: &mut ToolContext<'_>,
) -> Result<ToolLoopOutcome, (StatusCode, String)> {
    let visible_skills = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        None,
        None,
        1000,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skills for tool loop failed: {err}")))?;
    let visible_agents = agents_repo::list_agents(
        context.db,
        context.visible_user_ids.as_slice(),
        Some(true),
        200,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load agents for tool loop failed: {err}")))?;
    let visible_plugins = skills_repo::list_plugins_by_user_ids(
        context.db,
        context.visible_user_ids.as_slice(),
        300,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skill plugins for tool loop failed: {err}")))?;

    let mut messages = vec![
        json!({"role": "system", "content": build_tool_loop_system_prompt()}),
        json!({
            "role": "user",
            "content": build_tool_loop_user_prompt(
                context.request,
                visible_skills.as_slice(),
                visible_agents.as_slice(),
                visible_plugins.as_slice(),
            )
        }),
    ];
    let tools = build_agent_builder_tools();
    let mut created_agent = None;
    let mut final_content = None;

    for _ in 0..8 {
        let response =
            request_chat_completion(http, runtime, messages.as_slice(), Some(tools.as_slice()))
                .await?;
        let choice = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .ok_or_else(|| {
                bad_gateway_error(format!(
                    "agent builder response missing choices: {}",
                    response
                ))
            })?;
        let message = choice.get("message").cloned().ok_or_else(|| {
            bad_gateway_error(format!(
                "agent builder response missing message: {}",
                response
            ))
        })?;
        let message_content = extract_message_text(message.get("content"));
        let tool_calls = parse_tool_calls(message.get("tool_calls"));

        if !tool_calls.is_empty() {
            messages.push(json!({
                "role": "assistant",
                "content": message.get("content").cloned().unwrap_or(Value::Null),
                "tool_calls": tool_calls.iter().map(|item| item.raw.clone()).collect::<Vec<_>>(),
            }));

            for tool_call in tool_calls {
                let execution = execute_tool_call(context, &tool_call).await;
                let (success, payload, agent) = match execution {
                    Ok(result) => (true, result.payload, result.created_agent),
                    Err(err) => (false, json!({"error": err}), None),
                };
                if created_agent.is_none() {
                    created_agent = agent;
                }

                let tool_payload = json!({
                    "success": success,
                    "name": tool_call.name,
                    "data": payload,
                });
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call.id,
                    "content": serde_json::to_string(&tool_payload)
                        .unwrap_or_else(|_| tool_payload.to_string()),
                }));
            }
            continue;
        }

        if message_content
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            final_content = message_content;
            break;
        }

        if choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason == "stop")
            && created_agent.is_some()
        {
            break;
        }
    }

    if created_agent.is_none() {
        if let Some(content) = final_content.as_deref() {
            created_agent = create_agent_from_final_response(context, content).await?;
        }
    }

    Ok(ToolLoopOutcome {
        created_agent,
        final_content,
    })
}

async fn run_plain_json_fallback(
    http: &Client,
    runtime: &ModelRuntime,
    context: &mut ToolContext<'_>,
) -> Result<ToolLoopOutcome, (StatusCode, String)> {
    let visible_skills = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        None,
        None,
        1000,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skills for fallback failed: {err}")))?;
    let visible_agents = agents_repo::list_agents(
        context.db,
        context.visible_user_ids.as_slice(),
        Some(true),
        200,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load agents for fallback failed: {err}")))?;
    let visible_plugins = skills_repo::list_plugins_by_user_ids(
        context.db,
        context.visible_user_ids.as_slice(),
        300,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skill plugins for fallback failed: {err}")))?;
    context.state.listed_skills = true;

    let response = request_chat_completion(
        http,
        runtime,
        &[
            json!({"role": "system", "content": build_plain_system_prompt()}),
            json!({
                "role": "user",
                "content": build_plain_user_prompt(
                    context.request,
                    visible_skills.as_slice(),
                    visible_agents.as_slice(),
                    visible_plugins.as_slice(),
                ),
            }),
        ],
        None,
    )
    .await?;

    let content = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| extract_message_text(message.get("content")))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            bad_gateway_error(format!(
                "agent builder fallback returned empty content: {}",
                response
            ))
        })?;

    let created_agent = create_agent_from_final_response(context, content.as_str()).await?;
    Ok(ToolLoopOutcome {
        created_agent,
        final_content: Some(content),
    })
}

async fn create_agent_from_final_response(
    context: &mut ToolContext<'_>,
    raw: &str,
) -> Result<Option<MemoryAgent>, (StatusCode, String)> {
    let Some(parsed) = parse_json_candidate(raw) else {
        return Ok(None);
    };

    if let Some(agent_id) = parsed
        .get("created_agent_id")
        .or_else(|| parsed.get("agent_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let existing = agents_repo::get_agent_by_id(context.db, agent_id)
            .await
            .map_err(|err| internal_error(format!("load created agent failed: {err}")))?;
        return Ok(existing);
    }

    let payload = parsed
        .get("create_memory_agent")
        .cloned()
        .or_else(|| parsed.get("agent").cloned())
        .unwrap_or(parsed);
    let Some(object) = payload.as_object() else {
        return Ok(None);
    };

    let create_req = build_create_agent_request(context, object, false).await?;
    let created = agents_repo::create_agent(context.db, create_req)
        .await
        .map_err(|err| {
            internal_error(format!("create agent from fallback response failed: {err}"))
        })?;
    Ok(Some(created))
}
