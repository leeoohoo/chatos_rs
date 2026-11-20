// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod backend;
mod catalog;
mod contracts;
mod definition;
mod implementations;
mod product_skills;
mod skills;
mod system_tool_catalog;
mod tool_catalog;

pub use backend::{SystemMcpBackend, SystemMcpHost};
pub use catalog::{
    system_mcp_catalog, system_mcp_descriptor, system_mcp_descriptor_by_any,
    system_mcp_descriptor_by_embedded_kind, system_mcp_descriptor_by_resource_id,
    system_mcp_descriptor_by_server_name, system_mcp_descriptor_for_record, SystemMcpDescriptor,
};
pub use chatos_plugin_management_sdk::{
    SystemMcpKey, LEGACY_BUILTIN_MCP_RUNTIME_KIND, SYSTEM_MCP_RUNTIME_KIND,
};
pub use definition::{CatalogSystemMcpDefinition, SystemMcpDefinition};
pub use implementations::builtin::builtin_tool_catalog;
pub use implementations::{system_mcp_definition, system_mcp_definitions};
pub use product_skills::{
    product_skill_document, product_skill_documents, ProductSkillDocument, ProductSkillResource,
};
pub use skills::{
    system_mcp_product_skill_binding, system_mcp_provider_skills, SystemMcpProductSkillBinding,
    SystemMcpProviderSkill,
};
pub use system_tool_catalog::local_command_approval_decision_tool_definition;
pub use tool_catalog::{system_mcp_static_tools, system_mcp_tool_catalog, SystemMcpToolCatalog};
