// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use chatos_plugin_management_sdk::{
    PluginComponentKind, SkillActivationAttestationClaims, DEFAULT_SKILL_ACTIVATION_MAX_DEPTH,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::result::*;
use super::validation::*;
use super::{
    PluginComponentProvider, AGENT_TOOL_NAME, COMMAND_TOOL_NAME, SKILL_ACTIVATE_TOOL_NAME,
    SKILL_LIST_RESOURCES_TOOL_NAME, SKILL_READ_RESOURCE_TOOL_NAME,
};
use crate::providers::{ProviderCallError, ProviderCallOutcome};
use crate::runtime::{PluginLocalToolComponentBinding, RuntimeSessionSnapshot};

#[path = "local_runtime_skill.rs"]
mod skill_runtime;

pub(crate) fn skill_ref(binding: &PluginLocalToolComponentBinding) -> String {
    let digest = Sha256::digest(format!(
        "chatos.plugin.skill.ref.v2\n{}\n{}\n{}",
        binding.runtime.plugin_id,
        binding.runtime.release_id,
        binding.runtime.component.component_key,
    ));
    format!("SK{}", &hex::encode(digest)[..12])
}

fn skill_scope(
    snapshot: &RuntimeSessionSnapshot,
    binding: &PluginLocalToolComponentBinding,
) -> (String, String) {
    let (kind, material) = if let Some(project_id) = snapshot.project_id.as_deref() {
        (
            "project",
            format!(
                "project\n{}\n{}\n{}",
                snapshot.tenant_id, snapshot.owner_user_id, project_id
            ),
        )
    } else {
        (
            "user_public",
            format!(
                "user_public\n{}\n{}\n{}",
                snapshot.tenant_id, snapshot.owner_user_id, binding.device_id
            ),
        )
    };
    (
        kind.to_string(),
        hex::encode(Sha256::digest(material.as_bytes())),
    )
}

impl PluginComponentProvider {
    fn recovered_binding_key(snapshot: &RuntimeSessionSnapshot, resource_id: &str) -> String {
        format!("{}\n{resource_id}", snapshot.session_id)
    }

    async fn effective_binding(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        prepared: &PluginLocalToolComponentBinding,
    ) -> PluginLocalToolComponentBinding {
        self.recovered_bindings
            .read()
            .await
            .get(
                Self::recovered_binding_key(snapshot, prepared.runtime.resource_id.as_str())
                    .as_str(),
            )
            .cloned()
            .unwrap_or_else(|| prepared.clone())
    }

    async fn recover_binding(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        failed_binding: &PluginLocalToolComponentBinding,
    ) -> Result<PluginLocalToolComponentBinding, ProviderCallError> {
        let _guard = self.recovery_lock.lock().await;
        let resource_id = failed_binding.runtime.resource_id.as_str();
        let prepared = snapshot
            .plugin_local_tool_component_bindings
            .get(resource_id)
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Local tool component binding is missing",
                )
            })?;
        let current = self.effective_binding(snapshot, prepared).await;
        if current.adapter_session_id != failed_binding.adapter_session_id {
            return Ok(current);
        }
        let immutable = snapshot
            .plugin_tool_component_bindings
            .get(resource_id)
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "immutable Plugin tool component binding is missing",
                )
            })?;
        let route = snapshot
            .routes
            .iter()
            .find(|candidate| candidate.resource_id == resource_id)
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin tool component route is missing from the Runtime Session",
                )
            })?;
        let mut recovered = self
            .prepare_local(
                immutable,
                route,
                &snapshot.project_context,
                snapshot.session_id.as_str(),
                snapshot.owner_user_id.as_str(),
                snapshot.expires_at_unix,
            )
            .await?;
        // Tool publication and prepared instruction/static results belong to the immutable
        // Runtime Session snapshot. A direct component re-prepare validates the packaged
        // component again, then retains those original session-facing values.
        recovered.tools = prepared.tools.clone();
        recovered.instruction_items = prepared.instruction_items.clone();
        recovered.static_result = prepared.static_result.clone();
        validate_recovered_binding(prepared, &recovered)?;
        self.recovered_bindings.write().await.insert(
            Self::recovered_binding_key(snapshot, resource_id),
            recovered.clone(),
        );
        tracing::info!(
            session_id = snapshot.session_id.as_str(),
            resource_id,
            previous_adapter_session_id = failed_binding.adapter_session_id.as_str(),
            adapter_session_id = recovered.adapter_session_id.as_str(),
            "recovered Plugin Local tool component binding"
        );
        Ok(recovered)
    }

    fn skill_binding_by_ref<'a>(
        &self,
        snapshot: &'a RuntimeSessionSnapshot,
        requested_ref: &str,
    ) -> Result<&'a PluginLocalToolComponentBinding, ProviderCallError> {
        snapshot
            .plugin_local_tool_component_bindings
            .values()
            .find(|candidate| {
                candidate.runtime.skill_snapshot.is_some() && skill_ref(candidate) == requested_ref
            })
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Skill reference is not present in this Runtime Session catalog",
                )
            })
    }

    fn skill_binding_for_claims<'a>(
        &self,
        snapshot: &'a RuntimeSessionSnapshot,
        claims: &SkillActivationAttestationClaims,
    ) -> Result<&'a PluginLocalToolComponentBinding, ProviderCallError> {
        let binding = snapshot
            .plugin_local_tool_component_bindings
            .values()
            .find(|candidate| {
                candidate.runtime.plugin_id == claims.plugin_id
                    && candidate.runtime.release_id == claims.release_id
                    && candidate.runtime.component.component_key == claims.component_key
            })
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Skill activation component is not present in this Runtime Session",
                )
            })?;
        self.validate_skill_claims(
            snapshot,
            binding,
            claims,
            Some(claims.activation_ref.as_str()),
        )?;
        Ok(binding)
    }

    pub(super) async fn call_local(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let host_binding = snapshot
            .plugin_local_tool_component_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Local tool component binding is missing",
                )
            })?;
        validate_local_bound_route(snapshot, route, host_binding)?;
        if !host_binding.publishes_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is not published by the immutable Plugin component snapshot"
                    .to_string(),
            });
        }
        if host_binding.runtime.component.kind == PluginComponentKind::Command {
            ensure_expected_tool(original_tool_name, COMMAND_TOOL_NAME)?;
            let command_arguments = parse_command_arguments(arguments)?;
            if command_arguments != host_binding.runtime.command_arguments {
                return Err(ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: "Plugin Command arguments do not match the Runtime Session selection"
                        .to_string(),
                });
            }
            let result = host_binding.static_result.clone().ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "prepared Plugin Command has no approved instruction result",
                )
            })?;
            let response_bytes = serde_json::to_vec(&result)
                .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?
                .len();
            return Ok(ProviderCallOutcome {
                result,
                response_bytes,
            });
        }
        let (prepared_binding, verified_claims) = if host_binding.runtime.skill_snapshot.is_some() {
            match original_tool_name {
                SKILL_ACTIVATE_TOOL_NAME => {
                    let requested_ref = arguments
                        .get("skill_ref")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            ProviderCallError::invalid_response(
                                "Plugin Skill skill_ref is required",
                            )
                        })?;
                    (self.skill_binding_by_ref(snapshot, requested_ref)?, None)
                }
                SKILL_LIST_RESOURCES_TOOL_NAME | SKILL_READ_RESOURCE_TOOL_NAME => {
                    let activation = self
                        .active_skill_from_arguments(snapshot, &arguments)
                        .await?;
                    let binding = self.skill_binding_for_claims(snapshot, &activation.claims)?;
                    (binding, Some(activation.claims))
                }
                _ => {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin Skill runtime tool name is invalid",
                    ))
                }
            }
        } else {
            (host_binding, None)
        };
        let mut binding = self.effective_binding(snapshot, prepared_binding).await;
        let is_progressive_skill = binding.runtime.skill_snapshot.is_some();
        if is_progressive_skill && original_tool_name == SKILL_LIST_RESOURCES_TOOL_NAME {
            let claims = verified_claims.expect("resource listing verifies active Skill state");
            let resources = binding
                .runtime
                .skill_snapshot
                .as_ref()
                .unwrap()
                .resources
                .clone();
            let result = json!({
                "content": [{
                    "type": "text",
                    "text": format!(
                        "Resources for activated Skill {}:\n{}",
                        claims.skill_name,
                        serde_json::to_string_pretty(&resources).unwrap_or_else(|_| "[]".to_string())
                    )
                }],
                "structuredContent": {
                    "skill_ref": claims.skill_ref,
                    "resources": resources
                }
            });
            let response_bytes = serde_json::to_vec(&result)
                .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?
                .len();
            return Ok(ProviderCallOutcome {
                result,
                response_bytes,
            });
        }
        let execution_operation =
            if is_progressive_skill && original_tool_name == SKILL_ACTIVATE_TOOL_NAME {
                super::SKILL_ACTIVATE_OPERATION.to_string()
            } else if is_progressive_skill && original_tool_name == SKILL_READ_RESOURCE_TOOL_NAME {
                super::SKILL_READ_RESOURCE_OPERATION.to_string()
            } else if is_progressive_skill {
                return Err(ProviderCallError::invalid_response(
                    "Plugin Skill runtime tool name is invalid",
                ));
            } else {
                binding.operation.clone()
            };
        let relay_arguments = if execution_operation == super::SKILL_READ_RESOURCE_OPERATION {
            let relative_path = arguments
                .get("relative_path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ProviderCallError::invalid_response("Plugin Skill resource path is required")
                })?;
            let offset = arguments.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let max_chars = arguments
                .get("max_chars")
                .and_then(Value::as_u64)
                .unwrap_or(32_000)
                .clamp(1, 64_000);
            json!({
                "relative_path": relative_path,
                "offset": offset,
                "max_chars": max_chars,
            })
        } else {
            arguments
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}))
        };
        let mut body = serde_json::Map::from_iter([
            ("run_id".to_string(), json!(binding.run_id)),
            ("plugin_id".to_string(), json!(binding.runtime.plugin_id)),
            ("release_id".to_string(), json!(binding.runtime.release_id)),
            (
                "artifact_sha256".to_string(),
                json!(binding.runtime.artifact_sha256),
            ),
            (
                "component_key".to_string(),
                json!(binding.runtime.component.component_key),
            ),
            (
                "adapter_session_id".to_string(),
                json!(binding.adapter_session_id),
            ),
            ("invocation_id".to_string(), json!(invocation_id)),
            ("operation".to_string(), json!(execution_operation)),
        ]);
        if let Some(max_chars) = snapshot.tool_result_max_chars {
            body.insert("tool_result_max_chars".to_string(), json!(max_chars.max(1)));
        }
        match binding.runtime.component.kind {
            PluginComponentKind::SkillCollection => {
                if !arguments.is_object() {
                    return Err(ProviderCallError::invalid_response(
                        "native Plugin Skill tool arguments must be an object",
                    ));
                }
                body.insert("tool_name".to_string(), json!(original_tool_name));
                body.insert("arguments".to_string(), relay_arguments);
            }
            PluginComponentKind::Command => unreachable!("prepared commands return above"),
            PluginComponentKind::Agent => {
                ensure_expected_tool(original_tool_name, AGENT_TOOL_NAME)?;
                validate_empty_arguments(&arguments, "Plugin Agent apply")?;
                body.insert("arguments".to_string(), json!({}));
            }
            _ => {
                return Err(ProviderCallError::provider_unavailable(
                    "Plugin component kind is not callable",
                ))
            }
        }
        let first = self
            .request_local(
                snapshot.owner_user_id.as_str(),
                binding.device_id.as_str(),
                binding.workspace_id.as_deref(),
                snapshot
                    .project_context
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.relative_root.as_deref()),
                "execute",
                Value::Object(body.clone()),
            )
            .await;
        let bytes = match first {
            Ok(bytes) => bytes,
            Err(error) if is_recoverable_component_session_error(&error) => {
                binding = self.recover_binding(snapshot, &binding).await?;
                body.insert(
                    "adapter_session_id".to_string(),
                    json!(binding.adapter_session_id),
                );
                self.request_local(
                    snapshot.owner_user_id.as_str(),
                    binding.device_id.as_str(),
                    binding.workspace_id.as_deref(),
                    snapshot
                        .project_context
                        .workspace
                        .as_ref()
                        .and_then(|workspace| workspace.relative_root.as_deref()),
                    "execute",
                    Value::Object(body),
                )
                .await?
            }
            Err(error) => return Err(error),
        };
        let response: Value = serde_json::from_slice(bytes.as_slice()).map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Plugin Local component execute returned invalid JSON: {error}"
            ))
        })?;
        validate_execute_identity_for_operation(
            &binding,
            &response,
            execution_operation.as_str(),
            invocation_id,
        )?;
        let result = response.get("result").ok_or_else(|| {
            ProviderCallError::invalid_response(
                "Plugin Local component execute response is missing result",
            )
        })?;
        let progressive_skill = binding.runtime.skill_snapshot.as_ref();
        let result = match binding.runtime.component.kind {
            PluginComponentKind::SkillCollection
                if progressive_skill.is_some()
                    && execution_operation == super::SKILL_ACTIVATE_OPERATION =>
            {
                let instructions = validate_local_skill_activation(&binding.runtime, result)?;
                self.skill_activation_result(snapshot, &binding, &arguments, instructions.as_str())
                    .await?
            }
            PluginComponentKind::SkillCollection if progressive_skill.is_some() => {
                self.skill_resource_result(snapshot, &binding, &arguments, result)
                    .await?
            }
            PluginComponentKind::SkillCollection => result.clone(),
            PluginComponentKind::Command => unreachable!("prepared commands return above"),
            PluginComponentKind::Agent => {
                let agent = result.get("agent").ok_or_else(|| {
                    ProviderCallError::invalid_response(
                        "Plugin Agent invocation response is missing agent",
                    )
                })?;
                validate_agent_snapshot(&binding.runtime, agent)?;
                plugin_agent_result(&binding.runtime, agent)?
            }
            _ => unreachable!("validated local Plugin component kind"),
        };
        Ok(ProviderCallOutcome {
            result,
            response_bytes: bytes.len(),
        })
    }

    pub(super) fn validate_skill_claims(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        binding: &PluginLocalToolComponentBinding,
        claims: &SkillActivationAttestationClaims,
        expected_activation_ref: Option<&str>,
    ) -> Result<(), ProviderCallError> {
        let skill = binding.runtime.skill_snapshot.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable("Plugin Skill v2 snapshot is missing")
        })?;
        let (expected_scope_kind, expected_scope_id) = skill_scope(snapshot, binding);
        if claims.tenant_id != snapshot.tenant_id
            || claims.owner_user_id != snapshot.owner_user_id
            || claims.task_id != snapshot.task_id
            || claims.run_id != snapshot.run_id
            || claims.runtime_session_id != snapshot.session_id
            || claims.scope_kind != expected_scope_kind
            || claims.scope_id != expected_scope_id
            || claims.device_id.as_deref() != Some(binding.device_id.as_str())
            || claims.workspace_id != binding.workspace_id
            || claims.plugin_id != binding.runtime.plugin_id
            || claims.release_id != binding.runtime.release_id
            || claims.component_key != binding.runtime.component.component_key
            || claims.skill_ref != skill_ref(binding)
            || claims.skill_name != skill.metadata.name
            || expected_activation_ref.is_some_and(|expected| claims.activation_ref != expected)
            || claims.instructions_sha256 != skill.instructions_sha256
            || claims.resource_manifest_sha256 != skill.resource_manifest_sha256
        {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "Plugin Skill activation evidence does not match this Runtime Session"
                    .to_string(),
            });
        }
        Ok(())
    }
}

pub(super) fn is_recoverable_component_session_error(error: &ProviderCallError) -> bool {
    error.message.contains("Plugin Skill 会话不存在或已经结束")
        || error
            .message
            .contains("Plugin Skill session does not exist or has ended")
        || error.message.contains("no active control subscriber")
}

fn validate_recovered_binding(
    prepared: &PluginLocalToolComponentBinding,
    recovered: &PluginLocalToolComponentBinding,
) -> Result<(), ProviderCallError> {
    if recovered.runtime != prepared.runtime
        || recovered.run_id != prepared.run_id
        || recovered.device_id != prepared.device_id
        || recovered.workspace_id != prepared.workspace_id
        || recovered.operation != prepared.operation
        || recovered.tools != prepared.tools
        || recovered.instruction_items != prepared.instruction_items
        || recovered.static_result != prepared.static_result
    {
        return Err(ProviderCallError::invalid_response(
            "recovered Plugin Local tool component binding changed its immutable snapshot",
        ));
    }
    Ok(())
}

fn skill_activation_payload(
    skill: &chatos_plugin_management_sdk::PluginSkillComponentSnapshot,
    activation: &super::skill_attestation::ActiveSkillActivation,
    instructions: &str,
    deduplicated: bool,
) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": format!(
                "{}\n\n[Activated Plugin Skill: {}]\n{}\n\nThe platform has recorded this Skill activation for the current Runtime Session. Call the relevant Plugin tools with business arguments only; never add user, project, workspace, session, activation, or authentication fields.",
                super::THIRD_PARTY_PLUGIN_ENVELOPE,
                skill.metadata.name,
                instructions,
            )
        }],
        "structuredContent": {
            "activated": true,
            "skill_ref": activation.claims.skill_ref,
            "name": skill.metadata.name,
            "content_sha256": skill.instructions_sha256,
            "resource_manifest_sha256": skill.resource_manifest_sha256,
            "depth": activation.depth,
            "deduplicated": deduplicated,
            "expires_at_unix": activation.claims.expires_at_unix
        }
    })
}
