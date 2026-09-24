// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use chatos_mcp::{
    product_skill_document, PRODUCT_SKILL_LIST_RESOURCES_TOOL, PRODUCT_SKILL_READ_RESOURCE_TOOL,
    PRODUCT_SKILL_RUNTIME_RESOURCE_ID,
};
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use serde_json::{json, Map, Value};

use crate::runtime::RuntimeSessionSnapshot;

use super::{ProviderCallError, ProviderCallOutcome};

const PRODUCT_SKILL_REF_PREFIX: &str = "product-skill:";
const DEFAULT_PAGE_CHARACTERS: usize = 12_000;
const MAX_PAGE_CHARACTERS: usize = 20_000;

pub(super) fn supports(route: &ResolvedMcpRoute) -> bool {
    route.resource_id == PRODUCT_SKILL_RUNTIME_RESOURCE_ID && route.is_available()
}

pub(super) fn call_tool(
    snapshot: &RuntimeSessionSnapshot,
    original_tool_name: &str,
    arguments: Value,
) -> Result<ProviderCallOutcome, ProviderCallError> {
    let arguments = arguments.as_object().ok_or_else(|| {
        ProviderCallError::invalid_request("product Skill tool arguments must be an object")
    })?;
    let document = bound_document(snapshot, required_string(arguments, "skill_ref")?)?;
    let payload = match original_tool_name {
        PRODUCT_SKILL_LIST_RESOURCES_TOOL => json!({
            "skillRef": document.skill_ref(),
            "name": document.name,
            "instructionsSha256": document.instructions_sha256(),
            "resources": document.resources.iter().map(|resource| json!({
                "relativePath": resource.relative_path,
                "contentSha256": resource.content_sha256(),
                "sizeBytes": resource.size_bytes(),
            })).collect::<Vec<_>>(),
        }),
        PRODUCT_SKILL_READ_RESOURCE_TOOL => {
            let relative_path = required_string(arguments, "relative_path")?;
            validate_relative_path(relative_path)?;
            let resource = document.resource(relative_path).ok_or_else(|| {
                ProviderCallError::invalid_request(format!(
                    "resource is not declared by the bound product Skill: {relative_path}"
                ))
            })?;
            let expected_sha256 = required_string(arguments, "content_sha256")?;
            let content_sha256 = resource.content_sha256();
            if expected_sha256 != content_sha256 {
                return Err(ProviderCallError::invalid_request(
                    "product Skill resource content hash does not match the session manifest",
                ));
            }
            let offset = optional_usize(arguments, "offset")?.unwrap_or(0);
            let max_chars =
                optional_usize(arguments, "max_chars")?.unwrap_or(DEFAULT_PAGE_CHARACTERS);
            if max_chars == 0 || max_chars > MAX_PAGE_CHARACTERS {
                return Err(ProviderCallError::invalid_request(format!(
                    "max_chars must be between 1 and {MAX_PAGE_CHARACTERS}"
                )));
            }
            let total_chars = resource.content.chars().count();
            if offset > total_chars {
                return Err(ProviderCallError::invalid_request(format!(
                    "offset exceeds resource length: {offset} > {total_chars}"
                )));
            }
            let content = resource
                .content
                .chars()
                .skip(offset)
                .take(max_chars)
                .collect::<String>();
            let next_offset = offset + content.chars().count();
            json!({
                "skillRef": document.skill_ref(),
                "name": document.name,
                "relativePath": resource.relative_path,
                "contentSha256": content_sha256,
                "offset": offset,
                "nextOffset": next_offset,
                "totalChars": total_chars,
                "eof": next_offset == total_chars,
                "content": content,
            })
        }
        _ => {
            return Err(ProviderCallError::invalid_request(format!(
                "unknown product Skill control tool: {original_tool_name}"
            )))
        }
    };
    let text = serde_json::to_string(&payload).map_err(|error| {
        ProviderCallError::provider_unavailable(format!(
            "serialize product Skill result failed: {error}"
        ))
    })?;
    let result = json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": payload,
    });
    let response_bytes = serde_json::to_vec(&result)
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "measure product Skill result failed: {error}"
            ))
        })?
        .len();
    Ok(ProviderCallOutcome {
        result,
        response_bytes,
    })
}

fn bound_document(
    snapshot: &RuntimeSessionSnapshot,
    skill_ref: &str,
) -> Result<chatos_mcp::ProductSkillDocument, ProviderCallError> {
    let skill_ref = skill_ref.trim();
    let skill_name = skill_ref
        .strip_prefix(PRODUCT_SKILL_REF_PREFIX)
        .ok_or_else(|| {
            ProviderCallError::invalid_request(
                "skill_ref must be an exact product-skill: reference from protected context",
            )
        })?;
    if skill_name.is_empty() || skill_name.trim() != skill_name {
        return Err(ProviderCallError::invalid_request(
            "skill_ref contains an invalid product Skill name",
        ));
    }
    let allowed = snapshot
        .tools
        .iter()
        .filter_map(|tool| tool.skill_binding.as_ref())
        .flat_map(|binding| binding.required_skills.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    if !allowed.contains(skill_name) {
        return Err(ProviderCallError::invalid_request(format!(
            "product Skill is not bound to this Runtime Session: {skill_ref}"
        )));
    }
    product_skill_document(skill_name).ok_or_else(|| {
        ProviderCallError::provider_unavailable(format!(
            "bound product Skill is unavailable from the central bundle: {skill_name}"
        ))
    })
}

fn required_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, ProviderCallError> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ProviderCallError::invalid_request(format!("{field} is required")))
}

fn optional_usize(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<usize>, ProviderCallError> {
    let Some(value) = arguments.get(field) else {
        return Ok(None);
    };
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .map(Some)
        .ok_or_else(|| {
            ProviderCallError::invalid_request(format!("{field} must be a non-negative integer"))
        })
}

fn validate_relative_path(value: &str) -> Result<(), ProviderCallError> {
    if value.starts_with('/')
        || value.starts_with('\\')
        || value.split(['/', '\\']).any(|part| part == "..")
    {
        return Err(ProviderCallError::invalid_request(
            "relative_path must stay inside the declared product Skill resources",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chatos_mcp_management_sdk::{
        ProjectExecutionContext, RuntimeToolDescriptor, RuntimeToolSkillActivationPolicy,
        RuntimeToolSkillBinding, WorkspaceProviderKind,
    };

    use super::*;

    fn snapshot() -> RuntimeSessionSnapshot {
        RuntimeSessionSnapshot {
            session_id: "session-1".to_string(),
            caller_service: "task-runner".to_string(),
            trace_id: "trace-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_role: None,
            agent_key: "task_runner_agent".to_string(),
            task_profile: None,
            project_id: None,
            client_project_context: None,
            device_id: None,
            run_id: Some("run-1".to_string()),
            execution_group_id: None,
            execution_scope_generation: None,
            turn_id: None,
            task_id: None,
            task_title: None,
            source_session_id: None,
            source_user_message_id: None,
            contact_agent_id: None,
            default_model_config_id: None,
            default_remote_connection_id: None,
            remote_connection_route: None,
            tool_result_max_chars: None,
            workspace_route: None,
            project_context: ProjectExecutionContext {
                project_id: None,
                project_name: None,
                owner_user_id: "user-1".to_string(),
                workspace_provider: WorkspaceProviderKind::None,
                workspace: None,
                revision: "revision-1".to_string(),
            },
            policy_revision: "policy-1".to_string(),
            route_revision: "route-1".to_string(),
            routes: Vec::new(),
            tools: vec![RuntimeToolDescriptor {
                exposed_name: "remote_connection_run_command".to_string(),
                original_name: "run_command".to_string(),
                resource_id: "remote".to_string(),
                definition: json!({"name": "remote_connection_run_command"}),
                skill_binding: Some(RuntimeToolSkillBinding {
                    binding_id: "remote-connection".to_string(),
                    primary_skill: "chatos-remote-connection".to_string(),
                    required_skills: vec!["chatos-remote-connection".to_string()],
                    activation_policy: RuntimeToolSkillActivationPolicy::RunBound,
                    coverage_revision: 1,
                }),
            }],
            effective_mcp_ids: Vec::new(),
            provider_skills_prompt: None,
            plugin_instruction_items: Vec::new(),
            plugin_mcp_bindings: Default::default(),
            plugin_local_bindings: Default::default(),
            plugin_tool_component_bindings: Default::default(),
            plugin_local_tool_component_bindings: Default::default(),
            local_connector_mcp_bindings: Default::default(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            expires_at_unix: 4_070_908_800,
        }
    }

    #[test]
    fn lists_only_resources_for_a_bound_skill() {
        let outcome = call_tool(
            &snapshot(),
            PRODUCT_SKILL_LIST_RESOURCES_TOOL,
            json!({"skill_ref": "product-skill:chatos-remote-connection"}),
        )
        .expect("list resources");

        assert_eq!(
            outcome
                .result
                .pointer("/structuredContent/resources/0/relativePath")
                .and_then(Value::as_str),
            Some("references/commands-and-transfers.md")
        );
        assert!(call_tool(
            &snapshot(),
            PRODUCT_SKILL_LIST_RESOURCES_TOOL,
            json!({"skill_ref": "product-skill:chatos-project-write"}),
        )
        .is_err());
    }

    #[test]
    fn reads_unicode_pages_with_manifest_hash_validation() {
        let document = product_skill_document("chatos-remote-connection").expect("Skill");
        let resource = document.resources[0];
        let outcome = call_tool(
            &snapshot(),
            PRODUCT_SKILL_READ_RESOURCE_TOOL,
            json!({
                "skill_ref": document.skill_ref(),
                "relative_path": resource.relative_path,
                "content_sha256": resource.content_sha256(),
                "offset": 1,
                "max_chars": 7,
            }),
        )
        .expect("read resource");

        assert_eq!(
            outcome.result.pointer("/structuredContent/offset"),
            Some(&json!(1))
        );
        assert_eq!(
            outcome.result.pointer("/structuredContent/nextOffset"),
            Some(&json!(8))
        );
        assert_eq!(
            outcome
                .result
                .pointer("/structuredContent/content")
                .and_then(Value::as_str)
                .map(|value| value.chars().count()),
            Some(7)
        );
        assert!(call_tool(
            &snapshot(),
            PRODUCT_SKILL_READ_RESOURCE_TOOL,
            json!({
                "skill_ref": document.skill_ref(),
                "relative_path": resource.relative_path,
                "content_sha256": "0".repeat(64),
            }),
        )
        .is_err());
    }

    #[test]
    fn rejects_path_traversal_and_out_of_range_pages() {
        let document = product_skill_document("chatos-remote-connection").expect("Skill");
        let resource = document.resources[0];
        assert!(call_tool(
            &snapshot(),
            PRODUCT_SKILL_READ_RESOURCE_TOOL,
            json!({
                "skill_ref": document.skill_ref(),
                "relative_path": "references/../SKILL.md",
                "content_sha256": resource.content_sha256(),
            }),
        )
        .is_err());
        assert!(call_tool(
            &snapshot(),
            PRODUCT_SKILL_READ_RESOURCE_TOOL,
            json!({
                "skill_ref": document.skill_ref(),
                "relative_path": resource.relative_path,
                "content_sha256": resource.content_sha256(),
                "offset": resource.content.chars().count() + 1,
            }),
        )
        .is_err());
    }
}
