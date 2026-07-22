// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn resolve_agent_capabilities(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<RuntimeCapabilitiesQuery>,
) -> Result<Json<RuntimeCapabilitiesResponse>, ApiError> {
    let requested_owner = query
        .owner_user_id
        .as_deref()
        .and_then(|value| normalized(Some(value)));
    if !user.is_super_admin()
        && requested_owner
            .as_deref()
            .is_some_and(|owner| owner != user.effective_owner_user_id())
    {
        return Err(ApiError::forbidden(
            "ordinary users cannot resolve capabilities for another owner",
        ));
    }
    let owner_user_id = if user.is_super_admin() {
        requested_owner.unwrap_or_else(|| user.effective_owner_user_id().to_string())
    } else {
        user.effective_owner_user_id().to_string()
    };
    resolve_agent_capabilities_for_owner(
        &state,
        query.agent_key,
        owner_user_id,
        query.include_unavailable.unwrap_or(true),
        BindingConditions {
            task_profile: normalized(query.task_profile.as_deref()),
            project_source_type: normalized(query.project_source_type.as_deref()),
            runtime_provider: normalized(query.runtime_provider.as_deref()),
            schedule_mode: normalized(query.schedule_mode.as_deref()),
        },
    )
    .await
    .map(Json)
}

pub(super) async fn resolve_agent_capabilities_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RuntimeCapabilitiesRequest>,
) -> Result<Json<RuntimeCapabilitiesResponse>, ApiError> {
    let caller_service = require_internal_caller_service(&headers)?;
    require_internal_api_secret(&state, &headers, caller_service, CAPABILITIES_RESOLVE_SCOPE)?;
    let owner_user_id = normalized(Some(input.owner_user_id.as_str()))
        .ok_or_else(|| ApiError::bad_request("owner_user_id is required"))?;
    let agent_key = input.agent_key.to_string();
    tracing::debug!(
        caller_service,
        agent_key,
        "resolving agent capabilities through internal API"
    );
    resolve_agent_capabilities_for_owner(
        &state,
        agent_key,
        owner_user_id,
        input.include_unavailable,
        BindingConditions {
            task_profile: normalized(input.task_profile.as_deref()),
            project_source_type: normalized(input.project_source_type.as_deref()),
            runtime_provider: normalized(input.runtime_provider.as_deref()),
            schedule_mode: normalized(input.schedule_mode.as_deref()),
        },
    )
    .await
    .map(Json)
}

async fn resolve_agent_capabilities_for_owner(
    state: &AppState,
    agent_key: String,
    owner_user_id: String,
    include_unavailable: bool,
    runtime_context: BindingConditions,
) -> Result<RuntimeCapabilitiesResponse, ApiError> {
    let agent = state
        .store
        .get_agent(agent_key.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("System agent not found"))?;
    if !agent.enabled {
        return Err(ApiError::bad_request("System agent is disabled"));
    }
    let bindings = state
        .store
        .list_bindings_for_runtime(agent_key.as_str(), owner_user_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let mut mcps = Vec::new();
    let mut skills = Vec::new();
    let mut local_connector_requirements = Vec::new();

    for binding in bindings {
        if !binding_matches_runtime_context(&binding.conditions, &runtime_context) {
            continue;
        }
        match binding.resource_kind.as_str() {
            RESOURCE_KIND_MCP => {
                let Some(resource) = state
                    .store
                    .get_mcp(binding.resource_id.as_str())
                    .await
                    .map_err(ApiError::internal)?
                else {
                    continue;
                };
                if !resource_visible_in_runtime(
                    &resource.owner_user_id,
                    &resource.visibility,
                    owner_user_id.as_str(),
                    &binding,
                ) {
                    continue;
                }
                let (available, status, reason) = availability_for_mcp(state, &resource).await?;
                collect_local_connector_requirement_for_mcp(
                    &mut local_connector_requirements,
                    &resource,
                    &binding,
                    available,
                    reason.clone(),
                );
                if available || include_unavailable {
                    mcps.push(ResolvedMcp {
                        resource,
                        binding,
                        available,
                        status,
                        reason,
                    });
                }
            }
            RESOURCE_KIND_SKILL => {
                let Some(resource) = state
                    .store
                    .get_skill(binding.resource_id.as_str())
                    .await
                    .map_err(ApiError::internal)?
                else {
                    continue;
                };
                if !resource_visible_in_runtime(
                    &resource.owner_user_id,
                    &resource.visibility,
                    owner_user_id.as_str(),
                    &binding,
                ) {
                    continue;
                }
                if resource.content.kind == SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE
                    && !user_skill_enabled(state, owner_user_id.as_str(), resource.id.as_str())
                        .await?
                {
                    continue;
                }
                let (available, status, reason, installation) =
                    availability_for_skill(state, &resource, owner_user_id.as_str()).await?;
                collect_local_connector_requirement_for_skill(
                    &mut local_connector_requirements,
                    &resource,
                    &binding,
                    available,
                    reason.clone(),
                    installation.as_ref(),
                );
                if available || include_unavailable {
                    skills.push(ResolvedSkill {
                        resource,
                        binding,
                        available,
                        status,
                        reason,
                        installation,
                    });
                }
            }
            RESOURCE_KIND_SKILL_PACKAGE => {
                let Some(package) = state
                    .store
                    .get_skill_package(binding.resource_id.as_str())
                    .await
                    .map_err(ApiError::internal)?
                else {
                    continue;
                };
                if !package.installed
                    || !resource_visible_in_runtime(
                        &package.owner_user_id,
                        &package.visibility,
                        owner_user_id.as_str(),
                        &binding,
                    )
                {
                    continue;
                }
                for skill_id in &package.skill_ids {
                    let Some(resource) = state
                        .store
                        .get_skill(skill_id.as_str())
                        .await
                        .map_err(ApiError::internal)?
                    else {
                        continue;
                    };
                    if !resource_visible_in_runtime(
                        &resource.owner_user_id,
                        &resource.visibility,
                        owner_user_id.as_str(),
                        &binding,
                    ) {
                        continue;
                    }
                    if resource.content.kind == SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE
                        && !user_skill_enabled(state, owner_user_id.as_str(), resource.id.as_str())
                            .await?
                    {
                        continue;
                    }
                    let (available, status, reason, installation) =
                        availability_for_skill(state, &resource, owner_user_id.as_str()).await?;
                    collect_local_connector_requirement_for_skill(
                        &mut local_connector_requirements,
                        &resource,
                        &binding,
                        available,
                        reason.clone(),
                        installation.as_ref(),
                    );
                    if available || include_unavailable {
                        skills.push(ResolvedSkill {
                            resource,
                            binding: binding.clone(),
                            available,
                            status,
                            reason,
                            installation,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if agent.include_user_resources {
        let mut resolved_mcp_ids = mcps
            .iter()
            .map(|item| item.resource.id.clone())
            .collect::<HashSet<_>>();
        for resource in state
            .store
            .list_enabled_user_mcps(owner_user_id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if !resolved_mcp_ids.insert(resource.id.clone()) {
                continue;
            }
            let binding = automatic_user_binding(
                agent_key.as_str(),
                owner_user_id.as_str(),
                RESOURCE_KIND_MCP,
                resource.id.as_str(),
            );
            let (available, status, reason) = availability_for_mcp(state, &resource).await?;
            collect_local_connector_requirement_for_mcp(
                &mut local_connector_requirements,
                &resource,
                &binding,
                available,
                reason.clone(),
            );
            if available || include_unavailable {
                mcps.push(ResolvedMcp {
                    resource,
                    binding,
                    available,
                    status,
                    reason,
                });
            }
        }

        let mut resolved_skill_ids = skills
            .iter()
            .map(|item| item.resource.id.clone())
            .collect::<HashSet<_>>();
        for resource in state
            .store
            .list_enabled_user_skills(owner_user_id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if !resolved_skill_ids.insert(resource.id.clone()) {
                continue;
            }
            let binding = automatic_user_binding(
                agent_key.as_str(),
                owner_user_id.as_str(),
                RESOURCE_KIND_SKILL,
                resource.id.as_str(),
            );
            if resource.content.kind == SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE
                && !user_skill_enabled(state, owner_user_id.as_str(), resource.id.as_str()).await?
            {
                continue;
            }
            let (available, status, reason, installation) =
                availability_for_skill(state, &resource, owner_user_id.as_str()).await?;
            collect_local_connector_requirement_for_skill(
                &mut local_connector_requirements,
                &resource,
                &binding,
                available,
                reason.clone(),
                installation.as_ref(),
            );
            if available || include_unavailable {
                skills.push(ResolvedSkill {
                    resource,
                    binding,
                    available,
                    status,
                    reason,
                    installation,
                });
            }
        }
    }

    let generated_at = now_rfc3339();
    let policy_revision = capability_policy_revision(&agent, &mcps, &skills);
    Ok(RuntimeCapabilitiesResponse {
        agent_key,
        owner_user_id,
        policy_revision,
        generated_at,
        agent_enabled: agent.enabled,
        mcps,
        skills,
        local_connector_requirements,
    })
}

fn binding_matches_runtime_context(
    conditions: &BindingConditions,
    runtime_context: &BindingConditions,
) -> bool {
    condition_matches(
        conditions.task_profile.as_deref(),
        runtime_context.task_profile.as_deref(),
    ) && condition_matches(
        conditions.project_source_type.as_deref(),
        runtime_context.project_source_type.as_deref(),
    ) && condition_matches(
        conditions.runtime_provider.as_deref(),
        runtime_context.runtime_provider.as_deref(),
    ) && condition_matches(
        conditions.schedule_mode.as_deref(),
        runtime_context.schedule_mode.as_deref(),
    )
}

fn condition_matches(expected: Option<&str>, actual: Option<&str>) -> bool {
    let Some(expected) = normalized(expected) else {
        return true;
    };
    normalized(actual).is_some_and(|actual| actual.eq_ignore_ascii_case(expected.as_str()))
}

async fn user_skill_enabled(
    state: &AppState,
    owner_user_id: &str,
    skill_id: &str,
) -> Result<bool, ApiError> {
    state
        .store
        .get_user_skill_preference(owner_user_id, skill_id)
        .await
        .map(|record| record.is_some_and(|record| record.enabled))
        .map_err(ApiError::internal)
}

fn capability_policy_revision(
    agent: &SystemAgentRecord,
    mcps: &[ResolvedMcp],
    skills: &[ResolvedSkill],
) -> String {
    let mut revision_parts = vec![format!(
        "agent:{}:{}:{}",
        agent.agent_key, agent.enabled, agent.updated_at
    )];
    revision_parts.extend(mcps.iter().map(|item| {
        format!(
            "mcp:{}:{}:{}:{}:{}:{}",
            item.resource.id,
            item.resource.enabled,
            item.resource.updated_at,
            item.binding.required,
            item.binding.enabled,
            item.binding.updated_at
        )
    }));
    revision_parts.extend(skills.iter().map(|item| {
        format!(
            "skill:{}:{}:{}:{}:{}:{}",
            item.resource.id,
            item.resource.enabled,
            item.resource.updated_at,
            item.binding.required,
            item.binding.enabled,
            item.binding.updated_at
        )
    }));
    revision_parts.sort();
    let mut hasher = DefaultHasher::new();
    revision_parts.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub(super) fn automatic_user_binding(
    agent_key: &str,
    owner_user_id: &str,
    resource_kind: &str,
    resource_id: &str,
) -> AgentBindingRecord {
    let now = now_rfc3339();
    AgentBindingRecord {
        id: format!("{agent_key}__automatic_user__{resource_kind}__{resource_id}"),
        agent_key: agent_key.to_string(),
        binding_scope: BINDING_SCOPE_USER_OVERRIDE.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        resource_kind: resource_kind.to_string(),
        resource_id: resource_id.to_string(),
        enabled: true,
        required: false,
        priority: 1_000,
        conditions: BindingConditions::default(),
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: now.clone(),
        updated_at: now,
    }
}

#[cfg(test)]
mod condition_tests {
    use super::*;

    #[test]
    fn project_scoped_binding_only_matches_cloud_project_context() {
        let conditions = BindingConditions {
            project_source_type: Some("cloud".to_string()),
            ..BindingConditions::default()
        };
        assert!(binding_matches_runtime_context(
            &conditions,
            &BindingConditions {
                project_source_type: Some("CLOUD".to_string()),
                ..BindingConditions::default()
            }
        ));
        assert!(!binding_matches_runtime_context(
            &conditions,
            &BindingConditions {
                project_source_type: Some("public".to_string()),
                ..BindingConditions::default()
            }
        ));
        assert!(!binding_matches_runtime_context(
            &conditions,
            &BindingConditions::default()
        ));
    }

    #[test]
    fn unconditional_binding_matches_every_runtime_context() {
        assert!(binding_matches_runtime_context(
            &BindingConditions::default(),
            &BindingConditions {
                task_profile: Some("default".to_string()),
                project_source_type: Some("public".to_string()),
                schedule_mode: Some("contact_async".to_string()),
                ..BindingConditions::default()
            }
        ));
    }
}
