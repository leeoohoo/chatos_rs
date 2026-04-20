use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::mcp_tools::ToolStreamChunkCallback;
use crate::core::tool_io::text_result;
use crate::services::ui_prompt_manager::{
    create_ui_prompt_record, create_ui_prompt_request, normalize_choice_limits,
    normalize_choice_options, normalize_default_selection, normalize_kv_fields,
    redact_response_for_store, update_ui_prompt_response, wait_for_ui_prompt_decision, LimitMode,
    UiPromptDecision, UiPromptPayload, UiPromptResponseSubmission, UiPromptStatus,
    UI_PROMPT_TIMEOUT_ERR,
};
use crate::utils::events::Events;

use super::support::{
    build_mixed_choice_input, build_mixed_payload_map, choice_to_value, kv_fields_to_value,
    make_prompt_id, optional_string, parse_choice_block, parse_i64, parse_mixed_fields,
};
use super::ToolContext;

pub(super) fn handle_prompt_key_values(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let fields = normalize_kv_fields(args.get("fields"), 50)?;
    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        conversation_id: ctx.conversation_id.to_string(),
        conversation_turn_id: ctx.conversation_turn_id.to_string(),
        tool_call_id: None,
        kind: "kv".to_string(),
        title: optional_string(&args, "title"),
        message: optional_string(&args, "message"),
        allow_cancel: args
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: default_timeout_ms,
        payload: json!({
            "fields": kv_fields_to_value(fields.as_slice()),
        }),
    };

    let decision = execute_prompt(payload, ctx)?;
    Ok(text_result(json!({
        "status": decision.response.status,
        "values": decision.response.values.unwrap_or_else(|| json!({})),
    })))
}

pub(super) fn handle_prompt_choices(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let multiple = args
        .get("multiple")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let options = normalize_choice_options(args.get("options"), 60)?;
    let limits = normalize_choice_limits(
        multiple,
        parse_i64(args.get("min_selections")),
        parse_i64(args.get("max_selections")),
        options.len(),
        LimitMode::Clamp,
        parse_i64(args.get("single_min_selections")),
        parse_i64(args.get("single_max_selections")),
    )?;
    let default_selection =
        normalize_default_selection(args.get("default"), multiple, options.as_slice());

    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        conversation_id: ctx.conversation_id.to_string(),
        conversation_turn_id: ctx.conversation_turn_id.to_string(),
        tool_call_id: None,
        kind: "choice".to_string(),
        title: optional_string(&args, "title"),
        message: optional_string(&args, "message"),
        allow_cancel: args
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: default_timeout_ms,
        payload: json!({
            "choice": choice_to_value(multiple, options.as_slice(), &limits, default_selection),
        }),
    };

    let decision = execute_prompt(payload, ctx)?;
    Ok(text_result(json!({
        "status": decision.response.status,
        "selection": decision.response.selection.unwrap_or_else(|| {
            if multiple {
                Value::Array(Vec::new())
            } else {
                Value::String(String::new())
            }
        }),
    })))
}

pub(super) fn handle_prompt_mixed_form(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let fields = parse_mixed_fields(&args)?;
    let choice = parse_choice_block(build_mixed_choice_input(&args).as_ref())?;

    if fields.is_empty() && choice.is_none() {
        return Err("mixed form requires fields and/or choice".to_string());
    }

    let payload = UiPromptPayload {
        prompt_id: make_prompt_id(),
        conversation_id: ctx.conversation_id.to_string(),
        conversation_turn_id: ctx.conversation_turn_id.to_string(),
        tool_call_id: None,
        kind: "mixed".to_string(),
        title: optional_string(&args, "title"),
        message: optional_string(&args, "message"),
        allow_cancel: args
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: default_timeout_ms,
        payload: Value::Object(build_mixed_payload_map(fields.as_slice(), choice)),
    };

    let decision = execute_prompt(payload, ctx)?;
    Ok(text_result(json!({
        "status": decision.response.status,
        "values": decision.response.values.unwrap_or_else(|| json!({})),
        "selection": decision.response.selection.unwrap_or(Value::Null),
    })))
}

fn execute_prompt(payload: UiPromptPayload, ctx: &ToolContext) -> Result<UiPromptDecision, String> {
    block_on_result(create_ui_prompt_record(&payload))?;

    let (registered_payload, receiver) =
        block_on_result(create_ui_prompt_request(payload.clone()))?;
    emit_ui_prompt_required_event(ctx.on_stream_chunk.as_ref(), &registered_payload);

    let decision = match block_on_result(wait_for_ui_prompt_decision(
        registered_payload.prompt_id.as_str(),
        receiver,
        registered_payload.timeout_ms,
    )) {
        Ok(decision) => decision,
        Err(err) if err == UI_PROMPT_TIMEOUT_ERR => {
            let timeout_response = UiPromptResponseSubmission {
                status: UiPromptStatus::Timeout.as_str().to_string(),
                values: None,
                selection: None,
                reason: Some("timeout".to_string()),
            };
            let _ = block_on_result(update_ui_prompt_response(
                registered_payload.prompt_id.as_str(),
                UiPromptStatus::Timeout,
                Some(json!({
                    "status": "timeout",
                })),
            ));
            emit_ui_prompt_resolved_event(
                ctx.on_stream_chunk.as_ref(),
                registered_payload.prompt_id.as_str(),
                UiPromptStatus::Timeout,
            );
            return Ok(UiPromptDecision {
                status: UiPromptStatus::Timeout,
                response: timeout_response,
            });
        }
        Err(err) => return Err(err),
    };

    let redacted_response = redact_response_for_store(&decision.response, &registered_payload);
    let _ = block_on_result(update_ui_prompt_response(
        registered_payload.prompt_id.as_str(),
        decision.status,
        Some(redacted_response),
    ));
    emit_ui_prompt_resolved_event(
        ctx.on_stream_chunk.as_ref(),
        registered_payload.prompt_id.as_str(),
        decision.status,
    );
    Ok(decision)
}

fn emit_ui_prompt_required_event(
    on_stream_chunk: Option<&ToolStreamChunkCallback>,
    payload: &UiPromptPayload,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let chunk = json!({
        "event": Events::UI_PROMPT_REQUIRED,
        "data": payload,
    });
    if let Ok(serialized) = serde_json::to_string(&chunk) {
        callback(serialized);
    }
}

fn emit_ui_prompt_resolved_event(
    on_stream_chunk: Option<&ToolStreamChunkCallback>,
    prompt_id: &str,
    status: UiPromptStatus,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let chunk = json!({
        "event": Events::UI_PROMPT_RESOLVED,
        "data": {
            "prompt_id": prompt_id,
            "status": status.as_str(),
        }
    });
    if let Ok(serialized) = serde_json::to_string(&chunk) {
        callback(serialized);
    }
}
