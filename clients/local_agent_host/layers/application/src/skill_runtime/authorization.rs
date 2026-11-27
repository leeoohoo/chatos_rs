impl LocalAgentRuntime {
    #[doc(hidden)]
    pub async fn authorize_tool_skill_for_run(
        &self,
        run_id: &str,
        tool_name: &str,
    ) -> Result<Result<(), LocalToolSkillGateDenial>, LocalAgentRuntimeError> {
        self.authorize_tool_skill_invocation(run_id, tool_name, &serde_json::Value::Null)
            .await
    }

    #[doc(hidden)]
    pub async fn authorize_tool_skill_invocation(
        &self,
        run_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<Result<(), LocalToolSkillGateDenial>, LocalAgentRuntimeError> {
        // Routing is a Host control-plane decision that must precede optional Skill
        // activation. Keep historical frozen bundles from deadlocking if they grouped
        // this tool with on-demand Task operations.
        if tool_name == LOCAL_AGENT_DECIDE_TASK_ROUTE_TOOL_NAME {
            return Ok(Ok(()));
        }
        let Some((run, bundle)) = self.skill_bundle_for_run(run_id).await? else {
            // A profile without a Skill bundle has no Skill-gated tools. Model-visible
            // capability snapshots decide whether the Skill control plane exists at all.
            return Ok(Ok(()));
        };
        if run.profile_key == "task_execution"
            && bundle.schema_version >= 2
            && tool_name == "capability_invoke"
        {
            return self
                .authorize_discovered_tool_skill(&run, &bundle, arguments)
                .await;
        }
        let mut bindings = bundle
            .tool_skill_bindings
            .iter()
            .filter(|binding| binding.tool_names.iter().any(|name| name == tool_name));
        let Some(binding) = bindings.next() else {
            if bundle.schema_version >= 2 && tool_name.starts_with("external_mcp__") {
                return Ok(Err(LocalToolSkillGateDenial {
                    error: "skill_binding_required".into(),
                    tool_name: tool_name.into(),
                    binding_id: "external-mcp.missing-frozen-binding".into(),
                    next_action: None,
                    required_skill_refs: vec![],
                    bundle_revision: bundle.bundle_revision,
                }));
            }
            return Ok(Ok(()));
        };
        if bindings.next().is_some() {
            return Err(LocalAgentRuntimeError::InvalidRunState(format!(
                "tool is covered by multiple Skill bindings: {tool_name}"
            )));
        }
        match binding.activation_policy {
            LocalToolSkillActivationPolicy::ControlPlane
            | LocalToolSkillActivationPolicy::RunBound => Ok(Ok(())),
            LocalToolSkillActivationPolicy::OnDemand => {
                let required = [
                    binding.router_skill_ref.as_str(),
                    binding.specialist_skill_ref.as_str(),
                ]
                .into_iter()
                .collect::<BTreeSet<_>>();
                let activated = self
                    .activated_skill_refs_for_run(&run.owner_user_id, run_id)
                    .await?
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                let mut missing = Vec::new();
                for skill_ref in required {
                    if !activated.contains(skill_ref) {
                        missing.push(skill_ref.to_string());
                    }
                }
                if missing.is_empty() {
                    Ok(Ok(()))
                } else {
                    let batch = missing.iter().take(4).cloned().collect::<Vec<_>>();
                    Ok(Err(LocalToolSkillGateDenial {
                        error: "skill_activation_required".into(),
                        tool_name: tool_name.to_string(),
                        binding_id: binding.binding_id.clone(),
                        next_action: Some(serde_json::json!({
                            "tool_name": LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
                            "arguments": {"skill_refs": batch},
                            "then": "Activate any remaining required_skill_refs in batches of at most four, then retry the original invocation."
                        })),
                        required_skill_refs: missing,
                        bundle_revision: bundle.bundle_revision,
                    }))
                }
            }
        }
    }

    async fn authorize_discovered_tool_skill(
        &self,
        run: &LocalAgentRunRecord,
        bundle: &LocalSkillBundleSnapshot,
        arguments: &serde_json::Value,
    ) -> Result<Result<(), LocalToolSkillGateDenial>, LocalAgentRuntimeError> {
        let denial = |error: &str, required_skill_refs: Vec<String>| {
            let next_action = if error == "skill_activation_required"
                && !required_skill_refs.is_empty()
            {
                let batch = required_skill_refs
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>();
                Some(serde_json::json!({
                    "tool_name": LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
                    "arguments": {"skill_refs": batch},
                    "then": "Activate any remaining required_skill_refs in batches of at most four, then retry the original invocation."
                }))
            } else {
                arguments.get("plugin_option").and_then(serde_json::Value::as_str)
                    .zip(arguments.get("tool_option").and_then(serde_json::Value::as_str))
                    .map(|(plugin, tool)| serde_json::json!({
                        "tool_name": "capability_describe",
                        "arguments": {"plugin_option": plugin, "tool_option": tool},
                        "then": "Read this tool's full schema and Skill gate (a complete catalog entry is sufficient), activate required Skills, then retry with valid arguments."
                    }))
            };
            Ok(Err(LocalToolSkillGateDenial {
                error: error.into(),
                tool_name: "capability_invoke".into(),
                binding_id: "capability-broker.discovered-tool".into(),
                required_skill_refs,
                bundle_revision: bundle.bundle_revision.clone(),
                next_action,
            }))
        };
        let (Some(plugin_option), Some(tool_option), Some(tool_arguments)) = (
            arguments
                .get("plugin_option")
                .and_then(serde_json::Value::as_str),
            arguments
                .get("tool_option")
                .and_then(serde_json::Value::as_str),
            arguments.get("arguments").filter(|value| value.is_object()),
        ) else {
            return denial("skill_binding_invalid", vec![]);
        };
        let Some(result) = self
            .store
            .latest_successful_tool_invocation_result(
                &run.owner_user_id,
                &run.run_id,
                "capability_describe",
                "plugin_option",
                plugin_option,
                Some(tool_option),
            )
            .await?
        else {
            return denial("skill_binding_required", vec![]);
        };
        let descriptor = if result.get("is_error").is_some() {
            result.get("content").unwrap_or(&result)
        } else {
            &result
        };
        if descriptor
            .get("disclosure_version")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
            || !matches!(
                descriptor.get("view").and_then(serde_json::Value::as_str),
                Some("tool" | "catalog")
            )
            || descriptor
                .get("binding_schema_version")
                .and_then(serde_json::Value::as_u64)
                != Some(1)
            || descriptor
                .get("plugin_option")
                .and_then(serde_json::Value::as_str)
                != Some(plugin_option)
            || descriptor
                .get("provider_id")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|provider| {
                    !provider.starts_with("plugin:") || provider.len() <= "plugin:".len()
                })
        {
            return denial("skill_binding_required", vec![]);
        }
        let Some(tools) = descriptor
            .get("tools")
            .and_then(serde_json::Value::as_array)
        else {
            return denial("skill_binding_invalid", vec![]);
        };
        let matched = tools
            .iter()
            .filter(|tool| {
                tool.get("tool_option").and_then(serde_json::Value::as_str) == Some(tool_option)
            })
            .collect::<Vec<_>>();
        if matched.len() != 1
            || matched[0]
                .get("invocation_mode")
                .and_then(serde_json::Value::as_str)
                != Some("capability_invoke")
            || !complete_disclosed_tool(matched[0])
        {
            return denial("skill_binding_invalid", vec![]);
        }
        let Some(encoded_gate) = matched[0].get("skill_gate") else {
            return denial("skill_binding_invalid", vec![]);
        };
        let Ok(gate) = serde_json::from_value::<LocalDiscoveredToolSkillGate>(encoded_gate.clone())
        else {
            return denial("skill_binding_invalid", vec![]);
        };
        let Ok(required) = gate.required_skill_refs(tool_arguments) else {
            return denial("skill_binding_invalid", vec![]);
        };
        let expected_prefix = format!("plugin-skill:{plugin_option}:");
        if gate
            .all_of
            .iter()
            .chain(
                gate.select_by_argument
                    .iter()
                    .flat_map(|selector| selector.map.values()),
            )
            .any(|skill_ref| {
                !skill_ref.starts_with(&expected_prefix) || skill_ref.len() <= expected_prefix.len()
            })
        {
            return denial("skill_binding_invalid", vec![]);
        }
        // Catalog summaries declare every possible conditional ref. A malformed
        // native descriptor cannot turn an undeclared ref into a new permission.
        let Some(catalog) = matched[0]
            .get("required_skill_refs")
            .and_then(serde_json::Value::as_array)
        else {
            return denial("skill_binding_invalid", vec![]);
        };
        if required.iter().any(|skill_ref| {
            !catalog
                .iter()
                .any(|value| value.as_str() == Some(skill_ref))
        }) {
            return denial("skill_binding_invalid", vec![]);
        }
        let mut missing = vec![];
        let activated = self
            .activated_skill_refs_for_run(&run.owner_user_id, &run.run_id)
            .await?
            .into_iter()
            .collect::<BTreeSet<_>>();
        for skill_ref in required {
            if !activated.contains(&skill_ref) {
                missing.push(skill_ref);
            }
        }
        if missing.is_empty() {
            Ok(Ok(()))
        } else {
            denial("skill_activation_required", missing)
        }
    }
}
