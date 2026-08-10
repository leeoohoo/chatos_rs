// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::SystemMcpKey;

use super::CHATOS_MEMORY_PROVIDER_REF_PREFIX;

pub(crate) fn memory_provider_ref(contact_agent_id: &str) -> String {
    format!(
        "{CHATOS_MEMORY_PROVIDER_REF_PREFIX}{}",
        contact_agent_id.trim()
    )
}

pub(super) fn is_memory_reader(key: SystemMcpKey) -> bool {
    matches!(
        key,
        SystemMcpKey::MemorySkillReader
            | SystemMcpKey::MemoryCommandReader
            | SystemMcpKey::MemoryPluginReader
    )
}
