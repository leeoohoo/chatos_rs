// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    McpRecord, SystemMcpKey, CHATOS_TASK_RUNNER_MCP_RESOURCE_ID, LEGACY_BUILTIN_MCP_RUNTIME_KIND,
    LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID, PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
    PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID, SANDBOX_IMAGES_MCP_RESOURCE_ID,
    SYSTEM_MCP_RUNTIME_KIND,
};

use crate::{SystemMcpBackend, SystemMcpHost};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemMcpDescriptor {
    pub key: SystemMcpKey,
    pub resource_id: &'static str,
    pub server_name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub allow_writes: bool,
    pub tags: &'static [&'static str],
    pub category: Option<&'static str>,
    pub owner_service: &'static str,
    pub backend: SystemMcpBackend,
    pub supported_hosts: &'static [SystemMcpHost],
    pub embedded_kind: Option<BuiltinMcpKind>,
}

impl SystemMcpDescriptor {
    pub fn supports_host(self, host: SystemMcpHost) -> bool {
        self.supported_hosts.contains(&host)
    }

    pub const fn is_embedded(self) -> bool {
        matches!(self.backend, SystemMcpBackend::Embedded)
    }

    pub fn host_priority(self, host: SystemMcpHost) -> Option<u16> {
        if !self.supports_host(host) {
            return None;
        }
        if host == SystemMcpHost::LocalConnector {
            return Some(match self.key {
                SystemMcpKey::CodeMaintainerRead => 10,
                SystemMcpKey::CodeMaintainerWrite => 20,
                SystemMcpKey::TerminalController => 30,
                SystemMcpKey::BrowserTools => 40,
                SystemMcpKey::TaskManager => 50,
                SystemMcpKey::AskUser => 60,
                SystemMcpKey::ProjectManagement => 70,
                SystemMcpKey::TaskRunnerService => 80,
                SystemMcpKey::LocalCommandApproval => 90,
                _ => 1_000,
            });
        }
        SYSTEM_MCP_CATALOG
            .iter()
            .position(|descriptor| descriptor.key == self.key)
            .map(|index| (index as u16 + 1) * 10)
    }
}

const CHATOS_TASK_LOCAL_HOSTS: &[SystemMcpHost] = &[
    SystemMcpHost::Chatos,
    SystemMcpHost::TaskRunner,
    SystemMcpHost::LocalConnector,
];
const CHATOS_TASK_HOSTS: &[SystemMcpHost] = &[SystemMcpHost::Chatos, SystemMcpHost::TaskRunner];
const TASK_AND_LOCAL_HOSTS: &[SystemMcpHost] =
    &[SystemMcpHost::TaskRunner, SystemMcpHost::LocalConnector];
const TASK_RUNNER_HOST: &[SystemMcpHost] = &[SystemMcpHost::TaskRunner];
const CHATOS_HOST: &[SystemMcpHost] = &[SystemMcpHost::Chatos];
const CHATOS_AND_LOCAL_HOSTS: &[SystemMcpHost] =
    &[SystemMcpHost::Chatos, SystemMcpHost::LocalConnector];
const PROJECT_SERVICE_HOST: &[SystemMcpHost] = &[SystemMcpHost::ProjectManagementService];
const LOCAL_CONNECTOR_HOST: &[SystemMcpHost] = &[SystemMcpHost::LocalConnector];
const PROJECT_AND_SANDBOX_HOSTS: &[SystemMcpHost] = &[
    SystemMcpHost::ProjectManagementService,
    SystemMcpHost::SandboxManagerService,
];

macro_rules! embedded_descriptor {
    ($key:ident, $resource_id:expr, $server_name:expr, $display_name:expr, $description:expr, $allow_writes:expr, $owner:expr, $hosts:expr, $kind:ident) => {
        SystemMcpDescriptor {
            key: SystemMcpKey::$key,
            resource_id: $resource_id,
            server_name: $server_name,
            display_name: $display_name,
            description: $description,
            allow_writes: $allow_writes,
            tags: &["system", "builtin"],
            category: Some("builtin"),
            owner_service: $owner,
            backend: SystemMcpBackend::Embedded,
            supported_hosts: $hosts,
            embedded_kind: Some(BuiltinMcpKind::$kind),
        }
    };
}

static SYSTEM_MCP_CATALOG: [SystemMcpDescriptor; 19] = [
    embedded_descriptor!(
        CodeMaintainerRead,
        "builtin_code_maintainer_read",
        "code_maintainer_read",
        "Code Maintainer Read (Builtin)",
        "Read-only code inspection and search tools.",
        false,
        "shared",
        CHATOS_TASK_LOCAL_HOSTS,
        CodeMaintainerRead
    ),
    embedded_descriptor!(
        CodeMaintainerWrite,
        "builtin_code_maintainer_write",
        "code_maintainer_write",
        "Code Maintainer Write (Builtin)",
        "Code editing and patch application tools.",
        true,
        "shared",
        CHATOS_TASK_LOCAL_HOSTS,
        CodeMaintainerWrite
    ),
    embedded_descriptor!(
        TerminalController,
        "builtin_terminal_controller",
        "terminal_controller",
        "Terminal Controller (Builtin)",
        "Managed terminal execution and process lifecycle tools.",
        true,
        "shared",
        CHATOS_TASK_LOCAL_HOSTS,
        TerminalController
    ),
    embedded_descriptor!(
        TaskManager,
        "builtin_task_manager",
        "task_manager",
        "Task Manager (Builtin)",
        "Task planning, tracking, review, and completion tools.",
        true,
        "shared",
        CHATOS_TASK_LOCAL_HOSTS,
        TaskManager
    ),
    embedded_descriptor!(
        ProjectManagement,
        "builtin_project_management",
        "project_management_service",
        "Project Management (Builtin)",
        "Project, requirement, task, document, and dependency management tools.",
        true,
        "project_management_service",
        TASK_AND_LOCAL_HOSTS,
        ProjectManagement
    ),
    embedded_descriptor!(
        Notepad,
        "builtin_notepad",
        "notepad",
        "Notepad (Builtin)",
        "Persistent agent notepad tools.",
        true,
        "shared",
        CHATOS_TASK_HOSTS,
        Notepad
    ),
    embedded_descriptor!(
        AgentBuilder,
        "builtin_agent_builder",
        "agent_builder",
        "Agent Builder (Builtin)",
        "Agent configuration and skill composition tools.",
        true,
        "chatos",
        CHATOS_HOST,
        AgentBuilder
    ),
    embedded_descriptor!(
        AskUser,
        "builtin_ask_user",
        "ask_user",
        "Ask User (Builtin)",
        "Structured user clarification and decision tools.",
        true,
        "shared",
        CHATOS_TASK_LOCAL_HOSTS,
        AskUser
    ),
    embedded_descriptor!(
        RemoteConnectionController,
        "builtin_remote_connection_controller",
        "remote_connection_controller",
        "Remote Connection Controller (Builtin)",
        "Remote connection inspection and command tools.",
        true,
        "shared",
        CHATOS_TASK_HOSTS,
        RemoteConnectionController
    ),
    embedded_descriptor!(
        WebTools,
        "builtin_web_tools",
        "web_tools",
        "Web Tools (Builtin)",
        "Web research and content retrieval tools.",
        true,
        "shared",
        CHATOS_TASK_HOSTS,
        WebTools
    ),
    embedded_descriptor!(
        BrowserTools,
        "builtin_browser_tools",
        "browser_tools",
        "Browser Tools (Builtin)",
        "Interactive browser automation tools.",
        true,
        "shared",
        CHATOS_TASK_LOCAL_HOSTS,
        BrowserTools
    ),
    embedded_descriptor!(
        MemorySkillReader,
        "system_builtin_memory_skill_reader",
        "memory_skill_reader",
        "Memory Skill Reader (Builtin)",
        "Read agent skills from memory context.",
        false,
        "chatos",
        CHATOS_HOST,
        MemorySkillReader
    ),
    embedded_descriptor!(
        MemoryCommandReader,
        "system_builtin_memory_command_reader",
        "memory_command_reader",
        "Memory Command Reader (Builtin)",
        "Read agent commands from memory context.",
        false,
        "chatos",
        CHATOS_HOST,
        MemoryCommandReader
    ),
    embedded_descriptor!(
        MemoryPluginReader,
        "system_builtin_memory_plugin_reader",
        "memory_plugin_reader",
        "Memory Plugin Reader (Builtin)",
        "Read agent plugins from memory context.",
        false,
        "chatos",
        CHATOS_HOST,
        MemoryPluginReader
    ),
    SystemMcpDescriptor {
        key: SystemMcpKey::SandboxImages,
        resource_id: SANDBOX_IMAGES_MCP_RESOURCE_ID,
        server_name: "sandbox_images",
        display_name: "Sandbox Images",
        description: "Sandbox image discovery and preparation tools for project environments.",
        allow_writes: true,
        tags: &["system", "sandbox", "images"],
        category: Some("project_environment"),
        owner_service: "sandbox_manager_service",
        backend: SystemMcpBackend::HostAdapter,
        supported_hosts: PROJECT_AND_SANDBOX_HOSTS,
        embedded_kind: None,
    },
    SystemMcpDescriptor {
        key: SystemMcpKey::ProjectEnvironment,
        resource_id: PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
        server_name: "project_environment",
        display_name: "Project Environment",
        description:
            "Project environment state tools used by the Project Runtime Environment Agent.",
        allow_writes: true,
        tags: &["system", "project", "environment"],
        category: Some("project_environment"),
        owner_service: "project_management_service",
        backend: SystemMcpBackend::HostAdapter,
        supported_hosts: PROJECT_SERVICE_HOST,
        embedded_kind: None,
    },
    SystemMcpDescriptor {
        key: SystemMcpKey::ProjectRuntimeEnvironment,
        resource_id: PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
        server_name: "project_runtime_environment",
        display_name: "Project Runtime Environment",
        description:
            "Read-only initialized runtime environment information for Task Runner execution.",
        allow_writes: false,
        tags: &["system", "project", "runtime", "environment", "task_runner"],
        category: Some("task_runner"),
        owner_service: "project_management_service",
        backend: SystemMcpBackend::ServiceHttp,
        supported_hosts: TASK_RUNNER_HOST,
        embedded_kind: None,
    },
    SystemMcpDescriptor {
        key: SystemMcpKey::LocalCommandApproval,
        resource_id: LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
        server_name: "local_connector_approval",
        display_name: "Local Command Approval",
        description: "Final decision tools used by the Local Connector command approval agent.",
        allow_writes: true,
        tags: &["system", "local_connector", "approval"],
        category: Some("local_connector"),
        owner_service: "local_connector_client",
        backend: SystemMcpBackend::HostAdapter,
        supported_hosts: LOCAL_CONNECTOR_HOST,
        embedded_kind: None,
    },
    SystemMcpDescriptor {
        key: SystemMcpKey::TaskRunnerService,
        resource_id: CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        server_name: "task_runner_service",
        display_name: "Task Runner Service",
        description: "Task Runner MCP used by ChatOS to create and manage asynchronous tasks.",
        allow_writes: true,
        tags: &["system", "chatos", "task_runner"],
        category: Some("chatos"),
        owner_service: "task_runner_service",
        backend: SystemMcpBackend::ServiceDynamic,
        supported_hosts: CHATOS_AND_LOCAL_HOSTS,
        embedded_kind: None,
    },
];

pub fn system_mcp_catalog() -> &'static [SystemMcpDescriptor] {
    &SYSTEM_MCP_CATALOG
}

pub fn system_mcp_descriptor(key: SystemMcpKey) -> &'static SystemMcpDescriptor {
    SYSTEM_MCP_CATALOG
        .iter()
        .find(|descriptor| descriptor.key == key)
        .expect("every SystemMcpKey must have a descriptor")
}

pub fn system_mcp_descriptor_by_resource_id(value: &str) -> Option<&'static SystemMcpDescriptor> {
    let value = value.trim();
    SYSTEM_MCP_CATALOG
        .iter()
        .find(|descriptor| descriptor.resource_id == value)
}

pub fn system_mcp_descriptor_by_embedded_kind(
    kind: BuiltinMcpKind,
) -> Option<&'static SystemMcpDescriptor> {
    SYSTEM_MCP_CATALOG
        .iter()
        .find(|descriptor| descriptor.embedded_kind == Some(kind))
}

pub fn system_mcp_descriptor_by_server_name(value: &str) -> Option<&'static SystemMcpDescriptor> {
    let value = value.trim();
    SYSTEM_MCP_CATALOG
        .iter()
        .find(|descriptor| descriptor.server_name == value)
}

pub fn system_mcp_descriptor_by_any(value: &str) -> Option<&'static SystemMcpDescriptor> {
    let value = value.trim();
    system_mcp_descriptor_by_resource_id(value)
        .or_else(|| system_mcp_descriptor_by_server_name(value))
        .or_else(|| {
            value
                .parse::<SystemMcpKey>()
                .ok()
                .map(system_mcp_descriptor)
        })
        .or_else(|| {
            builtin_kind_by_any(value).and_then(|kind| {
                SYSTEM_MCP_CATALOG
                    .iter()
                    .find(|descriptor| descriptor.embedded_kind == Some(kind))
            })
        })
}

pub fn system_mcp_descriptor_for_record(
    record: &McpRecord,
) -> Option<&'static SystemMcpDescriptor> {
    if !matches!(
        record.runtime.kind.as_str(),
        SYSTEM_MCP_RUNTIME_KIND | LEGACY_BUILTIN_MCP_RUNTIME_KIND
    ) {
        return None;
    }
    record
        .runtime
        .system_key
        .as_deref()
        .or(record.runtime.builtin_kind.as_deref())
        .or(record.runtime.server_name.as_deref())
        .and_then(system_mcp_descriptor_by_any)
        .or_else(|| system_mcp_descriptor_by_any(record.id.as_str()))
        .or_else(|| system_mcp_descriptor_by_any(record.name.as_str()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_contains_every_system_key_once() {
        let keys = system_mcp_catalog()
            .iter()
            .map(|descriptor| descriptor.key)
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), SystemMcpKey::ALL.len());
        assert_eq!(
            keys.iter().copied().collect::<HashSet<_>>().len(),
            keys.len()
        );
    }

    #[test]
    fn resource_ids_and_server_names_are_unique() {
        let resources = system_mcp_catalog()
            .iter()
            .map(|descriptor| descriptor.resource_id)
            .collect::<HashSet<_>>();
        let servers = system_mcp_catalog()
            .iter()
            .map(|descriptor| descriptor.server_name)
            .collect::<HashSet<_>>();
        assert_eq!(resources.len(), system_mcp_catalog().len());
        assert_eq!(servers.len(), system_mcp_catalog().len());
    }

    #[test]
    fn legacy_builtin_identifiers_resolve_to_system_descriptors() {
        let descriptor = system_mcp_descriptor_by_any("CodeMaintainerRead").expect("descriptor");
        assert_eq!(descriptor.key, SystemMcpKey::CodeMaintainerRead);
        assert_eq!(
            system_mcp_descriptor_by_any("builtin_code_maintainer_read").map(|item| item.key),
            Some(SystemMcpKey::CodeMaintainerRead)
        );
    }

    #[test]
    fn system_record_resolution_supports_new_and_legacy_builtin_runtime_shapes() {
        use chatos_plugin_management_sdk::{McpRuntime, ResourceMetadata, ResourceSecurity};

        let record = |kind: &str, system_key: Option<&str>, builtin_kind: Option<&str>| McpRecord {
            id: "builtin_code_maintainer_read".to_string(),
            owner_user_id: "system".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: "code_maintainer_read".to_string(),
            display_name: "Code Maintainer Read".to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: kind.to_string(),
                system_key: system_key.map(ToOwned::to_owned),
                builtin_kind: builtin_kind.map(ToOwned::to_owned),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        assert_eq!(
            system_mcp_descriptor_for_record(&record(
                SYSTEM_MCP_RUNTIME_KIND,
                Some("code_maintainer_read"),
                None,
            ))
            .map(|descriptor| descriptor.key),
            Some(SystemMcpKey::CodeMaintainerRead)
        );
        assert_eq!(
            system_mcp_descriptor_for_record(&record(
                LEGACY_BUILTIN_MCP_RUNTIME_KIND,
                None,
                Some("CodeMaintainerRead"),
            ))
            .map(|descriptor| descriptor.key),
            Some(SystemMcpKey::CodeMaintainerRead)
        );
    }
}
