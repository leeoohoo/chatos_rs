// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_ports::LocalMemoryOutboxStore;
use chatos_local_agent_protocol::{GetMemorySyncStatusCommand, LOCAL_AGENT_PROTOCOL_VERSION};

#[tokio::test]
async fn routes_tenant_scoped_memory_sync_status() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    storage
        .enqueue_memory_record(
            "message-1",
            "user-1",
            "local_agent",
            "conversation-1",
            &json!({"message_id": "message-1"}),
            1_000,
        )
        .await
        .expect("enqueue");
    let runtime = LocalAgentRuntime::new(storage);
    let result = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "memory-status-1".to_string(),
            command: HostCommand::GetMemorySyncStatus(GetMemorySyncStatusCommand {
                tenant_id: "user-1".to_string(),
                source_id: "local_agent".to_string(),
            }),
        })
        .await
        .expect("status");
    assert!(matches!(
        result,
        HostResult::MemorySyncStatus { status }
            if status.pending_count == 1 && status.unsynced_count() == 1
    ));
}
