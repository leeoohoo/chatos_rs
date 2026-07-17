// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_ai_runtime::{
    RuntimeBeforeModelRequest, RuntimeFinalResponseAction, RuntimeFinalResponseContext,
    RuntimeIterationContext, RuntimeLifecycleHook,
};
use serde_json::{json, Value};

use crate::local_now_rfc3339;
use crate::local_runtime::storage::LocalDatabase;

use super::{LocalRuntimeGuidance, LocalTurnControlRegistry};

#[derive(Clone)]
pub(crate) struct LocalGuidanceLifecycleHook {
    registry: LocalTurnControlRegistry,
    database: LocalDatabase,
    owner_user_id: String,
    session_id: String,
    turn_id: String,
}

impl LocalGuidanceLifecycleHook {
    pub(crate) fn new(
        registry: LocalTurnControlRegistry,
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            registry,
            database,
            owner_user_id: owner_user_id.into(),
            session_id: session_id.into(),
            turn_id: turn_id.into(),
        }
    }

    async fn drain_input_items(&self) -> Vec<Value> {
        let guidance = self
            .registry
            .drain_guidance(self.session_id.as_str(), self.turn_id.as_str());
        for item in &guidance {
            let applied_at = local_now_rfc3339();
            let _ = self
                .database
                .mark_guidance_applied(
                    self.owner_user_id.as_str(),
                    item.message_id.as_str(),
                    applied_at.as_str(),
                )
                .await;
        }
        guidance.into_iter().map(guidance_input_item).collect()
    }
}

#[async_trait]
impl RuntimeLifecycleHook for LocalGuidanceLifecycleHook {
    async fn before_model_request(
        &self,
        _context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        Ok(RuntimeBeforeModelRequest::unchanged().with_input_items(self.drain_input_items().await))
    }

    async fn after_final_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        let guidance_items = self.drain_input_items().await;
        if guidance_items.is_empty() {
            Ok(RuntimeFinalResponseAction::Accept)
        } else {
            let mut input_items = vec![assistant_response_item(context.response.content.as_str())];
            input_items.extend(guidance_items);
            Ok(RuntimeFinalResponseAction::Continue {
                input_items,
                reason: "runtime_guidance".to_string(),
            })
        }
    }
}

fn assistant_response_item(content: &str) -> Value {
    json!({
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": content,
        }]
    })
}

fn guidance_input_item(item: LocalRuntimeGuidance) -> Value {
    json!({
        "role": "system",
        "content": format!(
            "[Runtime Guidance]\n- source: user guidance during the active local turn\n- instruction: {}\n- rule: treat this as a high-priority preference unless it conflicts with safety",
            item.content
        )
    })
}
