// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::{
        fetch_conversation, fetch_conversation_record_for_owner, insert_user_message_on,
    },
    memory_outbox_store, run_commands, ClientStorageError, IdempotentCommand, SqliteClientStorage,
    SqliteResultExt,
};
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, GuideConversationTurnCommand, LocalAgentRunRecord,
    LocalConversationAttachmentSpec, LocalConversationTurnUpdate, ResumeConversationTurnCommand,
    LOCAL_AGENT_MAX_INPUT_BYTES,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

include!("conversation_commands/commands.rs");
include!("conversation_commands/guidance.rs");
include!("conversation_commands/memory_projection.rs");
include!("conversation_commands/updates.rs");
#[cfg(test)]
mod guidance_size_tests {
    use super::*;

    #[test]
    fn continuation_limit_counts_guidance_but_not_tool_results() {
        let guidance = json!({"message_id":"follow-up","message":"continue","attachments":[]});
        let continuation = json!({
            "type":"tool_results",
            "invocations":[{"result_json":{"image":"x".repeat(700_000)}}],
            "guidance":[guidance.clone()]
        });
        let encoded = serde_json::to_string(&continuation).unwrap();
        assert_eq!(
            continuation_guidance_bytes(Some(&encoded)).unwrap(),
            serde_json::to_vec(&guidance).unwrap().len()
        );
        assert_eq!(
            continuation_guidance_bytes(Some(
                &serde_json::to_string(&json!({
                    "type":"tool_results",
                    "invocations":[{"result_json":{"image":"x".repeat(700_000)}}]
                }))
                .unwrap()
            ))
            .unwrap(),
            0
        );
    }
}
