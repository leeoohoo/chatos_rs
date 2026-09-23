use super::*;

impl PluginComponentProvider {
    pub(super) async fn active_skill_from_arguments(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        arguments: &Value,
    ) -> Result<
        crate::providers::plugin_components::skill_attestation::ActiveSkillActivation,
        ProviderCallError,
    > {
        let requested_skill_ref = arguments
            .get("skill_ref")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::invalid_response("Plugin Skill skill_ref is required")
            })?;
        let activation = self
            .skill_attestations
            .active_for_skill_ref(snapshot.session_id.as_str(), requested_skill_ref)
            .await
            .map_err(ProviderCallError::provider_unavailable)?
            .ok_or_else(|| ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "Plugin Skill is not active in this Runtime Session".to_string(),
            })?;
        let binding = self.skill_binding_for_claims(snapshot, &activation.claims)?;
        self.validate_skill_claims(snapshot, binding, &activation.claims, None)?;
        Ok(activation)
    }

    pub(super) async fn skill_resource_result(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        binding: &PluginLocalToolComponentBinding,
        arguments: &Value,
        result: &Value,
    ) -> Result<Value, ProviderCallError> {
        let claims = self
            .active_skill_from_arguments(snapshot, arguments)
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
                "skill_ref": claims.skill_ref,
                "relative_path": requested_path,
                "sha256": descriptor.sha256,
                "offset": result.get("offset").cloned().unwrap_or(json!(0)),
                "next_offset": result.get("next_offset").cloned().unwrap_or(Value::Null),
                "truncated": result.get("truncated").cloned().unwrap_or(json!(false))
            }
        }))
    }

    pub(super) async fn skill_activation_result(
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
        let mut parent_candidates = Vec::new();
        for activation in self
            .skill_attestations
            .active_activations(snapshot.session_id.as_str())
            .await
            .map_err(ProviderCallError::provider_unavailable)?
        {
            if activation.claims.plugin_id != binding.runtime.plugin_id
                || activation.claims.release_id != binding.runtime.release_id
                || activation.claims.skill_name == skill.metadata.name
            {
                continue;
            }
            let parent_binding = self.skill_binding_for_claims(snapshot, &activation.claims)?;
            let parent_skill = parent_binding.runtime.skill_snapshot.as_ref().unwrap();
            if parent_skill
                .metadata
                .required_skills
                .iter()
                .chain(parent_skill.metadata.related_skills.iter())
                .any(|name| name == &skill.metadata.name)
            {
                parent_candidates.push(activation);
            }
        }
        parent_candidates.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then(left.claims.issued_at_unix.cmp(&right.claims.issued_at_unix))
                .then(left.claims.activation_ref.cmp(&right.claims.activation_ref))
        });
        let parent = parent_candidates.pop();
        let parent_activation_ref = parent
            .as_ref()
            .map(|activation| activation.claims.activation_ref.clone());
        let depth = if let Some(parent) = parent {
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
        claims.activation_ref =
            crate::providers::plugin_components::skill_attestation::new_activation_reference();
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
