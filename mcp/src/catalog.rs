// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    McpRecord, SystemMcpKey, CHATOS_TASK_RUNNER_MCP_RESOURCE_ID, LEGACY_BUILTIN_MCP_RUNTIME_KIND,
    LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID, SYSTEM_MCP_RUNTIME_KIND,
    TASK_PROCESS_LOG_MCP_RESOURCE_ID,
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
    /// Hosts that contain a concrete provider implementation for this system MCP.
    /// Agent runtime routing is owned by MCP Management Service.
    pub implementation_hosts: &'static [SystemMcpHost],
    pub embedded_kind: Option<BuiltinMcpKind>,
}

impl SystemMcpDescriptor {
    pub fn supports_implementation_host(self, host: SystemMcpHost) -> bool {
        self.implementation_hosts.contains(&host)
    }

    pub const fn is_embedded(self) -> bool {
        matches!(self.backend, SystemMcpBackend::Embedded)
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
const CHATOS_HOST: &[SystemMcpHost] = &[SystemMcpHost::Chatos];
const CHATOS_AND_LOCAL_HOSTS: &[SystemMcpHost] =
    &[SystemMcpHost::Chatos, SystemMcpHost::LocalConnector];
const LOCAL_CONNECTOR_HOST: &[SystemMcpHost] = &[SystemMcpHost::LocalConnector];

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
            implementation_hosts: $hosts,
            embedded_kind: Some(BuiltinMcpKind::$kind),
        }
    };
}

static SYSTEM_MCP_CATALOG: [SystemMcpDescriptor; 14] = [
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
        TASK_AND_LOCAL_HOSTS,
        TerminalController
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
    SystemMcpDescriptor {
        key: SystemMcpKey::Notepad,
        resource_id: "builtin_notepad",
        server_name: "notepad",
        display_name: "Notepad (Builtin)",
        description: "Persistent agent notepad tools backed by the cloud ChatOS user store.",
        allow_writes: true,
        tags: &["system", "builtin"],
        category: Some("builtin"),
        owner_service: "chatos",
        backend: SystemMcpBackend::ServiceHttp,
        implementation_hosts: CHATOS_TASK_HOSTS,
        embedded_kind: Some(BuiltinMcpKind::Notepad),
    },
    SystemMcpDescriptor {
        key: SystemMcpKey::AgentBuilder,
        resource_id: "builtin_agent_builder",
        server_name: "agent_builder",
        display_name: "Agent Builder (Builtin)",
        description: "Owner-scoped agent configuration and skill composition tools.",
        allow_writes: true,
        tags: &["system", "builtin"],
        category: Some("builtin"),
        owner_service: "chatos",
        backend: SystemMcpBackend::ServiceHttp,
        implementation_hosts: CHATOS_HOST,
        embedded_kind: Some(BuiltinMcpKind::AgentBuilder),
    },
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
    SystemMcpDescriptor {
        key: SystemMcpKey::RemoteConnectionController,
        resource_id: "builtin_remote_connection_controller",
        server_name: "remote_connection_controller",
        display_name: "Remote Connection Controller (Builtin)",
        description: "Owner-scoped remote connection inspection, file, and command tools executed by Local Connector.",
        allow_writes: true,
        tags: &["system", "builtin", "remote_connection"],
        category: Some("builtin"),
        owner_service: "local_connector_client",
        backend: SystemMcpBackend::HostAdapter,
        implementation_hosts: LOCAL_CONNECTOR_HOST,
        embedded_kind: Some(BuiltinMcpKind::RemoteConnectionController),
    },
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
        implementation_hosts: LOCAL_CONNECTOR_HOST,
        embedded_kind: None,
    },
    SystemMcpDescriptor {
        key: SystemMcpKey::TaskProcessLog,
        resource_id: TASK_PROCESS_LOG_MCP_RESOURCE_ID,
        server_name: "task_run_process",
        display_name: "Task Process Log",
        description:
            "Run-scoped Task Runner MCP for recording short visible execution breadcrumbs on the current task.",
        allow_writes: true,
        tags: &["system", "task_runner", "process_log", "run_scoped"],
        category: Some("task_runner"),
        owner_service: "task_runner_service",
        backend: SystemMcpBackend::RunScopedBuiltin,
        implementation_hosts: TASK_AND_LOCAL_HOSTS,
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
        implementation_hosts: CHATOS_AND_LOCAL_HOSTS,
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
    fn task_process_log_resolves_as_run_scoped_system_mcp() {
        let descriptor =
            system_mcp_descriptor_by_any("task_run_process").expect("task process log descriptor");

        assert_eq!(descriptor.key, SystemMcpKey::TaskProcessLog);
        assert_eq!(descriptor.backend, SystemMcpBackend::RunScopedBuiltin);
        assert!(descriptor.supports_implementation_host(SystemMcpHost::TaskRunner));
        assert!(descriptor.supports_implementation_host(SystemMcpHost::LocalConnector));
    }

    #[test]
    fn terminal_controller_executes_only_in_task_runner_or_local_connector() {
        let descriptor = system_mcp_descriptor(SystemMcpKey::TerminalController);

        assert!(!descriptor.supports_implementation_host(SystemMcpHost::Chatos));
        assert!(descriptor.supports_implementation_host(SystemMcpHost::TaskRunner));
        assert!(descriptor.supports_implementation_host(SystemMcpHost::LocalConnector));
    }

    #[test]
    fn notepad_is_owned_by_the_chatos_cloud_service() {
        let descriptor = system_mcp_descriptor(SystemMcpKey::Notepad);

        assert_eq!(descriptor.owner_service, "chatos");
        assert_eq!(descriptor.backend, SystemMcpBackend::ServiceHttp);
        assert_eq!(descriptor.embedded_kind, Some(BuiltinMcpKind::Notepad));
        assert!(descriptor.supports_implementation_host(SystemMcpHost::Chatos));
        assert!(descriptor.supports_implementation_host(SystemMcpHost::TaskRunner));
    }

    #[test]
    fn agent_builder_is_owned_by_the_chatos_cloud_service() {
        let descriptor = system_mcp_descriptor(SystemMcpKey::AgentBuilder);

        assert_eq!(descriptor.owner_service, "chatos");
        assert_eq!(descriptor.backend, SystemMcpBackend::ServiceHttp);
        assert_eq!(descriptor.embedded_kind, Some(BuiltinMcpKind::AgentBuilder));
        assert!(descriptor.supports_implementation_host(SystemMcpHost::Chatos));
        assert!(!descriptor.supports_implementation_host(SystemMcpHost::TaskRunner));
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
            plugin_component: Default::default(),
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
