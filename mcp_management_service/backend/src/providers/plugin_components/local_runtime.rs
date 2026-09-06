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
        let (binding, verified_claims) = if host_binding.runtime.skill_snapshot.is_some() {
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
                    let activation = self.verify_skill_evidence(snapshot, &arguments).await?;
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
        let progressive_skill = binding.runtime.skill_snapshot.as_ref();
        if progressive_skill.is_some() && original_tool_name == SKILL_LIST_RESOURCES_TOOL_NAME {
            let claims = verified_claims.expect("resource listing verifies activation evidence");
            let resources = progressive_skill.unwrap().resources.clone();
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
                    "activation_ref": claims.activation_ref,
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
        let execution_operation = if progressive_skill.is_some()
            && original_tool_name == SKILL_ACTIVATE_TOOL_NAME
        {
            super::SKILL_ACTIVATE_OPERATION
        } else if progressive_skill.is_some() && original_tool_name == SKILL_READ_RESOURCE_TOOL_NAME
        {
            super::SKILL_READ_RESOURCE_OPERATION
        } else if progressive_skill.is_some() {
            return Err(ProviderCallError::invalid_response(
                "Plugin Skill runtime tool name is invalid",
            ));
        } else {
            binding.operation.as_str()
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
        let bytes = self
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
                Value::Object(body),
            )
            .await?;
        let response: Value = serde_json::from_slice(bytes.as_slice()).map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Plugin Local component execute returned invalid JSON: {error}"
            ))
        })?;
        validate_execute_identity_for_operation(
            binding,
            &response,
            execution_operation,
            invocation_id,
        )?;
        let result = response.get("result").ok_or_else(|| {
            ProviderCallError::invalid_response(
                "Plugin Local component execute response is missing result",
            )
        })?;
        let result = match binding.runtime.component.kind {
            PluginComponentKind::SkillCollection
                if progressive_skill.is_some()
                    && execution_operation == super::SKILL_ACTIVATE_OPERATION =>
            {
                let instructions = validate_local_skill_activation(&binding.runtime, result)?;
                self.skill_activation_result(snapshot, binding, &arguments, instructions.as_str())
                    .await?
            }
            PluginComponentKind::SkillCollection if progressive_skill.is_some() => {
                self.skill_resource_result(snapshot, binding, &arguments, result)
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

    async fn verify_skill_evidence(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        arguments: &Value,
    ) -> Result<super::skill_attestation::ActiveSkillActivation, ProviderCallError> {
        let token = arguments
            .get("activation_evidence")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::invalid_response("Plugin Skill activation evidence is required")
            })?;
        let activation = self
            .skill_attestations
            .verify_active(token)
            .await
            .map_err(ProviderCallError::provider_unavailable)?;
        let requested_activation_ref = arguments
            .get("activation_ref")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let binding = self.skill_binding_for_claims(snapshot, &activation.claims)?;
        self.validate_skill_claims(
            snapshot,
            binding,
            &activation.claims,
            Some(requested_activation_ref),
        )?;
        Ok(activation)
    }

    async fn skill_resource_result(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        binding: &PluginLocalToolComponentBinding,
        arguments: &Value,
        result: &Value,
    ) -> Result<Value, ProviderCallError> {
        let claims = self
            .verify_skill_evidence(snapshot, arguments)
            .await?
            .claims;
        let skill = binding.runtime.skill_snapshot.as_ref().unwrap();
        let requested_path = arguments
            .get("relative_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let descriptor = skill
            .resources
            .iter()
            .find(|resource| resource.relative_path == requested_path)
            .ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin Skill resource is not present in the immutable catalog",
                )
            })?;
        if result.get("skill_id").and_then(Value::as_str) != Some(skill.skill_id.as_str())
            || result.get("relative_path").and_then(Value::as_str) != Some(requested_path)
            || result.get("sha256").and_then(Value::as_str) != Some(descriptor.sha256.as_str())
            || result.get("content").and_then(Value::as_str).is_none()
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin Skill resource response does not match the immutable catalog",
            ));
        }
        let content = result.get("content").and_then(Value::as_str).unwrap();
        Ok(json!({
            "content": [{"type": "text", "text": content}],
            "structuredContent": {
                "activation_ref": claims.activation_ref,
                "skill_ref": claims.skill_ref,
                "relative_path": requested_path,
                "sha256": descriptor.sha256,
                "offset": result.get("offset").cloned().unwrap_or(json!(0)),
                "next_offset": result.get("next_offset").cloned().unwrap_or(Value::Null),
                "truncated": result.get("truncated").cloned().unwrap_or(json!(false))
            }
        }))
    }

    async fn skill_activation_result(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        binding: &PluginLocalToolComponentBinding,
        arguments: &Value,
        instructions: &str,
    ) -> Result<Value, ProviderCallError> {
        let skill = binding.runtime.skill_snapshot.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable("Plugin Skill v2 snapshot is missing")
        })?;
        let arguments_value = arguments
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let arguments_sha256 =
            crate::providers::canonical_json::canonical_json_sha256(&arguments_value)
                .map_err(ProviderCallError::invalid_response)?;
        let skill_ref = skill_ref(binding);
        let now = chrono::Utc::now().timestamp();
        let expires_at = snapshot.expires_at_unix.min(now + 60 * 60);
        let (scope_kind, scope_id) = skill_scope(snapshot, binding);
        let parent_activation_ref = arguments
            .get("parent_activation_ref")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let depth = if let Some(parent_ref) = parent_activation_ref.as_deref() {
            let parent = self
                .skill_attestations
                .activation(snapshot.session_id.as_str(), parent_ref)
                .await
                .map_err(ProviderCallError::provider_unavailable)?
                .ok_or_else(|| {
                    ProviderCallError::provider_unavailable(
                        "parent Skill activation is not active in this Runtime Session",
                    )
                })?;
            if parent.claims.plugin_id != binding.runtime.plugin_id
                || parent.claims.release_id != binding.runtime.release_id
            {
                return Err(ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: "parent Skill activation belongs to another Plugin Release"
                        .to_string(),
                });
            }
            let parent_binding = self.skill_binding_for_claims(snapshot, &parent.claims)?;
            let parent_skill = parent_binding.runtime.skill_snapshot.as_ref().unwrap();
            if !parent_skill
                .metadata
                .required_skills
                .iter()
                .chain(parent_skill.metadata.related_skills.iter())
                .any(|name| name == &skill.metadata.name)
            {
                return Err(ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: format!(
                        "parent Skill {} does not declare {} as a required or related Skill",
                        parent_skill.metadata.name, skill.metadata.name
                    ),
                });
            }
            let parent_depth = parent.depth;
            let mut cursor = Some(parent);
            while let Some(ancestor) = cursor {
                if ancestor.claims.skill_name == skill.metadata.name {
                    return Err(ProviderCallError::provider_unavailable(
                        "Plugin Skill activation cycle is not allowed",
                    ));
                }
                cursor = match ancestor.parent_activation_ref.as_deref() {
                    Some(reference) => self
                        .skill_attestations
                        .activation(snapshot.session_id.as_str(), reference)
                        .await
                        .map_err(ProviderCallError::provider_unavailable)?,
                    None => None,
                };
            }
            parent_depth.saturating_add(1)
        } else {
            0
        };
        if depth > DEFAULT_SKILL_ACTIVATION_MAX_DEPTH {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Plugin Skill activation depth exceeds {DEFAULT_SKILL_ACTIVATION_MAX_DEPTH}"
            )));
        }
        for required_name in &skill.metadata.required_skills {
            let available =
                snapshot
                    .plugin_local_tool_component_bindings
                    .values()
                    .any(|candidate| {
                        candidate.runtime.plugin_id == binding.runtime.plugin_id
                            && candidate.runtime.release_id == binding.runtime.release_id
                            && candidate
                                .runtime
                                .skill_snapshot
                                .as_ref()
                                .is_some_and(|value| value.metadata.name == *required_name)
                    });
            if !available {
                return Err(ProviderCallError::provider_unavailable(format!(
                    "required Plugin Skill is unavailable in this Runtime Session: {required_name}"
                )));
            }
        }
        let mut claims = SkillActivationAttestationClaims {
            issuer: "mcp-management-service".to_string(),
            audience: "plugin-skill-runtime".to_string(),
            tenant_id: snapshot.tenant_id.clone(),
            owner_user_id: snapshot.owner_user_id.clone(),
            task_id: snapshot.task_id.clone(),
            run_id: snapshot.run_id.clone(),
            runtime_session_id: snapshot.session_id.clone(),
            scope_kind,
            scope_id,
            device_id: Some(binding.device_id.clone()),
            workspace_id: binding.workspace_id.clone(),
            plugin_id: binding.runtime.plugin_id.clone(),
            release_id: binding.runtime.release_id.clone(),
            component_key: binding.runtime.component.component_key.clone(),
            skill_ref: skill_ref.clone(),
            skill_name: skill.metadata.name.clone(),
            activation_ref: String::new(),
            instructions_sha256: skill.instructions_sha256.clone(),
            resource_manifest_sha256: skill.resource_manifest_sha256.clone(),
            arguments_sha256,
            nonce: Uuid::new_v4().simple().to_string(),
            issued_at_unix: now,
            expires_at_unix: expires_at,
        };
        if let Some(existing) = self
            .skill_attestations
            .find_equivalent(&claims, parent_activation_ref.as_deref())
            .await
            .map_err(ProviderCallError::provider_unavailable)?
        {
            return Ok(skill_activation_payload(
                skill,
                &existing,
                instructions,
                true,
            ));
        }
        claims.activation_ref = format!("SA{}", Uuid::new_v4().simple());
        let activation = self
            .skill_attestations
            .register(
                claims,
                parent_activation_ref,
                depth,
                instructions.to_string(),
            )
            .await
            .map_err(ProviderCallError::provider_unavailable)?;
        Ok(skill_activation_payload(
            skill,
            &activation,
            instructions,
            false,
        ))
    }

    pub(in crate::providers) async fn close_local_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalToolComponentBinding>,
    ) {
        for binding in bindings.values() {
            let body = json!({
                "run_id": binding.run_id,
                "plugin_id": binding.runtime.plugin_id,
                "release_id": binding.runtime.release_id,
                "artifact_sha256": binding.runtime.artifact_sha256,
                "component_key": binding.runtime.component.component_key,
                "adapter_session_id": binding.adapter_session_id,
            });
            if let Err(error) = self
                .request_local(
                    owner_user_id,
                    binding.device_id.as_str(),
                    binding.workspace_id.as_deref(),
                    None,
                    "cancel",
                    body,
                )
                .await
            {
                tracing::warn!(
                    session_id = runtime_session_id,
                    resource_id = binding.runtime.resource_id.as_str(),
                    error = error.message,
                    "close Plugin Local tool component session failed"
                );
            }
        }
        if let Err(error) = self
            .skill_attestations
            .remove_session(runtime_session_id)
            .await
        {
            tracing::warn!(
                session_id = runtime_session_id,
                error,
                "remove Plugin Skill activation state failed"
            );
        }
    }
}

fn skill_activation_payload(
    skill: &chatos_plugin_management_sdk::PluginSkillComponentSnapshot,
    activation: &super::skill_attestation::ActiveSkillActivation,
    instructions: &str,
    deduplicated: bool,
) -> Value {
    let activation_receipt = json!({
        "activation_ref": activation.claims.activation_ref,
        "activation_evidence": activation.evidence,
        "skill_ref": activation.claims.skill_ref,
        "name": skill.metadata.name,
        "parent_activation_ref": activation.parent_activation_ref,
        "depth": activation.depth,
    });
    json!({
        "content": [{
            "type": "text",
            "text": format!(
                "{}\n\n[Activated Plugin Skill: {}]\n{}\n\n[Plugin Skill Activation Receipt]\n{}\nUse activation_evidence only as an exact gated-tool argument. Never edit it, substitute activation_ref for it, or disclose it in user-facing output.",
                super::THIRD_PARTY_PLUGIN_ENVELOPE,
                skill.metadata.name,
                instructions,
                serde_json::to_string(&activation_receipt).unwrap_or_else(|_| "{}".to_string()),
            )
        }],
        "structuredContent": {
            "activation_ref": activation.claims.activation_ref,
            "activation_evidence": activation.evidence,
            "skill_ref": activation.claims.skill_ref,
            "name": skill.metadata.name,
            "content_sha256": skill.instructions_sha256,
            "resource_manifest_sha256": skill.resource_manifest_sha256,
            "parent_activation_ref": activation.parent_activation_ref,
            "depth": activation.depth,
            "deduplicated": deduplicated,
            "expires_at_unix": activation.claims.expires_at_unix
        }
    })
}
