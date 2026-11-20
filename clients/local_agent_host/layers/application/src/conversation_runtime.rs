// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{new_event_id, LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{
    AuthoritativeConversationRecord, ClientStorageError, IdempotentCommand,
};
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, HostCommand, HostResult,
    LocalAgentRunRecord, LocalAgentRunStatus, LocalConversationAttachmentRecord,
    LocalConversationAttachmentSpec, LocalConversationDetail, LocalConversationHistoryPage,
    LocalConversationMessageRecord, LocalConversationMessageRole, LocalConversationRecord,
    LocalConversationResourceBinding, LocalConversationResourceKind, LocalConversationTurnRecord,
    LocalConversationTurnStatus, ResumeConversationTurnCommand, StartConversationTurnCommand,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::{HashMap, HashSet};
const JIGULI_AGENT_ID: &str = "jiguli";

include!("conversation_runtime/commands.rs");
include!("conversation_runtime/history.rs");
include!("conversation_runtime/agent_context.rs");
include!("conversation_runtime/team_context.rs");
include!("conversation_runtime/capability_selection.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_ports::{
        AuthoritativeConversationPage, LocalConversationTranscriptSource, LocalMemoryOutboxStore,
    };
    use chatos_local_agent_protocol::{
        CancelConversationTurnCommand, ClaimNextRunCommand, CommitStepCommand,
        GetConversationCommand, GetConversationHistoryCommand, GuideConversationTurnCommand,
        HostRequestEnvelope, ListConversationsCommand, LocalAgentStepOutcome,
        LocalConversationAttachmentSpec, LocalConversationMessageRole, LocalConversationTurnStatus,
        ResumeConversationTurnCommand, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use std::sync::Arc;

    include!("conversation_runtime/tests/fixtures.rs");
    include!("conversation_runtime/tests/history.rs");
    include!("conversation_runtime/tests/context.rs");
    include!("conversation_runtime/tests/commands.rs");
}
