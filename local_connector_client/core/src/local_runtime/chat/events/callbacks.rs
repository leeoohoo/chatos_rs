// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_ai_runtime::RuntimeCallbacks;
use serde_json::{json, Value};

use super::publisher::LocalChatEventSender;

pub(in crate::local_runtime) fn runtime_callbacks(
    sender: LocalChatEventSender,
) -> RuntimeCallbacks {
    RuntimeCallbacks {
        on_chunk: Some(Arc::new({
            let sender = sender.clone();
            move |text| sender.publish("chat.chunk", Some("text"), json!({ "text": text }))
        })),
        on_thinking: Some(Arc::new({
            let sender = sender.clone();
            move |text| sender.publish("chat.thinking", Some("reasoning"), json!({ "text": text }))
        })),
        on_tools_start: Some(Arc::new({
            let sender = sender.clone();
            move |payload| {
                sender.publish(
                    "chat.tools.start",
                    Some("tool"),
                    compact_tool_calls(payload),
                )
            }
        })),
        on_tools_stream: Some(Arc::new({
            let sender = sender.clone();
            move |payload| {
                let browser_session = browser_session_event_payload(&payload);
                sender.publish(
                    "chat.tools.stream",
                    Some("tool"),
                    compact_tool_result(payload),
                );
                if let Some(browser_session) = browser_session {
                    sender.publish("browser_session", Some("browser"), browser_session);
                }
            }
        })),
        on_tools_end: Some(Arc::new({
            let sender = sender.clone();
            move |payload| {
                let results = payload
                    .get("tool_results")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .cloned()
                            .map(compact_tool_result)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                sender.publish(
                    "chat.tools.end",
                    Some("tool"),
                    json!({ "tool_results": results }),
                )
            }
        })),
        on_turn_phase: Some(Arc::new({
            let sender = sender.clone();
            move |payload| sender.publish("chat.phase", Some("status"), payload)
        })),
        on_runtime_guidance_applied: Some(Arc::new(move |payload| {
            sender.publish("chat.guidance.applied", Some("status"), payload)
        })),
        ..RuntimeCallbacks::default()
    }
}

fn compact_tool_calls(payload: Value) -> Value {
    Value::Array(
        payload
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|call| {
                json!({
                    "tool_call_id": call.get("call_id").or_else(|| call.get("id")),
                    "name": call.get("name").or_else(|| {
                        call.get("function").and_then(|function| function.get("name"))
                    }),
                })
            })
            .collect(),
    )
}

fn compact_tool_result(payload: Value) -> Value {
    json!({
        "tool_call_id": payload.get("tool_call_id"),
        "name": payload.get("name"),
        "success": payload.get("success"),
        "is_error": payload.get("is_error"),
        "is_stream": payload.get("is_stream"),
    })
}

fn browser_session_event_payload(value: &Value) -> Option<Value> {
    match value {
        Value::Object(map) => {
            if let Some(session) = map.get("browser_session").filter(|value| value.is_object()) {
                return Some(session.clone());
            }
            map.values().find_map(browser_session_event_payload)
        }
        Value::Array(items) => items.iter().find_map(browser_session_event_payload),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::browser_session_event_payload;

    #[test]
    fn extracts_nested_browser_session_payload() {
        let payload = json!({
            "name": "browser_navigate",
            "result": {
                "_structured_result": {
                    "browser_session": {
                        "id": "h_session",
                        "mode": "managed",
                        "workspace_id": "workspace-1"
                    }
                }
            }
        });

        let session = browser_session_event_payload(&payload).expect("browser session");
        assert_eq!(session["id"], "h_session");
        assert_eq!(session["mode"], "managed");
        assert_eq!(session["workspace_id"], "workspace-1");
    }
}
