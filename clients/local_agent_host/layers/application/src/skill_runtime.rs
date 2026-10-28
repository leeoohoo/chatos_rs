// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_protocol::{
    LocalAgentRunRecord, LocalDiscoveredToolSkillGate, LocalSkillBundleSnapshot,
    LocalToolSkillActivationPolicy, LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
    LOCAL_SKILL_BUNDLE_PROFILE_KEY, LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY,
};
use std::collections::BTreeSet;

fn complete_disclosed_tool(tool: &serde_json::Value) -> bool {
    let valid_digest = tool
        .get("schema_sha256")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| {
            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    valid_digest
        && tool
            .get("invocation_mode")
            .and_then(serde_json::Value::as_str)
            == Some("capability_invoke")
        && (tool.get("input_schema").is_some()
            || (tool
                .get("schema_delivery")
                .and_then(serde_json::Value::as_str)
                == Some("paged")
                && tool
                    .get("schema_chunk")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
                && tool
                    .get("next_schema_offset")
                    .is_some_and(serde_json::Value::is_null)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalToolSkillGateDenial {
    pub error: String,
    pub tool_name: String,
    pub binding_id: String,
    pub required_skill_refs: Vec<String>,
    pub bundle_revision: String,
    pub next_action: Option<serde_json::Value>,
}

impl LocalAgentRuntime {
    #[doc(hidden)]
    pub async fn skill_bundle_for_run(
        &self,
        run_id: &str,
    ) -> Result<Option<(LocalAgentRunRecord, LocalSkillBundleSnapshot)>, LocalAgentRuntimeError>
    {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        let bundle_profile_key = if run.profile_key == "task_execution" {
            LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY
        } else {
            LOCAL_SKILL_BUNDLE_PROFILE_KEY
        };
        let snapshot = self
            .store
            .get_capability_snapshot(
                &run.owner_user_id,
                bundle_profile_key,
                &run.capability_policy_revision,
            )
            .await?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let encoded = snapshot.instructions.ok_or_else(|| {
            LocalAgentRuntimeError::InvalidRunState(
                "Skill bundle snapshot is missing its encoded bundle".to_string(),
            )
        })?;
        let bundle: LocalSkillBundleSnapshot = serde_json::from_str(&encoded).map_err(|error| {
            LocalAgentRuntimeError::InvalidRunState(format!(
                "Skill bundle snapshot is invalid: {error}"
            ))
        })?;
        bundle
            .validate()
            .map_err(LocalAgentRuntimeError::InvalidRunState)?;
        Ok(Some((run, bundle)))
    }

    #[doc(hidden)]
    pub async fn skill_is_activated(
        &self,
        run_id: &str,
        skill_ref: &str,
    ) -> Result<bool, LocalAgentRuntimeError> {
        Ok(self
            .store
            .has_successful_tool_invocation_argument(
                run_id,
                LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
                "skill_ref",
                skill_ref,
            )
            .await?)
    }

    #[doc(hidden)]
    pub async fn activated_skill_refs_for_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Vec<String>, LocalAgentRuntimeError> {
        let mut skill_refs = self
            .store
            .successful_tool_invocation_argument_values(
                owner_user_id,
                run_id,
                LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
                "skill_ref",
            )
            .await?;
        skill_refs.sort();
        skill_refs.dedup();
        Ok(skill_refs)
    }

    #[doc(hidden)]
    pub async fn capability_disclosures_for_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<
        Vec<chatos_local_agent_protocol::LocalRunCapabilityDisclosure>,
        LocalAgentRuntimeError,
    > {
        let invocations = self
            .store
            .successful_tool_invocations_for_run(owner_user_id, run_id, "capability_describe")
            .await?;
        let mut disclosures = std::collections::BTreeSet::new();
        for invocation in invocations {
            let Some(plugin_option) = invocation
                .arguments
                .get("plugin_option")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let Some(result) = invocation.result.as_ref() else {
                continue;
            };
            let descriptor = result.get("content").unwrap_or(result);
            let view = descriptor.get("view").and_then(serde_json::Value::as_str);
            if descriptor
                .get("disclosure_version")
                .and_then(serde_json::Value::as_u64)
                != Some(2)
                || !matches!(view, Some("tool" | "catalog"))
                || descriptor
                    .get("plugin_option")
                    .and_then(serde_json::Value::as_str)
                    != Some(plugin_option)
            {
                continue;
            }
            let Some(provider_id) = descriptor
                .get("provider_id")
                .and_then(serde_json::Value::as_str)
                .filter(|value| value.starts_with("plugin:") && value.len() > "plugin:".len())
            else {
                continue;
            };
            let Some(tools) = descriptor
                .get("tools")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            let requested_tool_option = (view == Some("tool"))
                .then(|| {
                    invocation
                        .arguments
                        .get("tool_option")
                        .and_then(serde_json::Value::as_str)
                })
                .flatten();
            if view == Some("tool") && requested_tool_option.is_none() {
                continue;
            }
            for tool in tools {
                let Some(tool_option) = tool.get("tool_option").and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                if requested_tool_option.is_some_and(|requested| requested != tool_option)
                    || tools
                        .iter()
                        .filter(|candidate| {
                            candidate
                                .get("tool_option")
                                .and_then(serde_json::Value::as_str)
                                == Some(tool_option)
                        })
                        .count()
                        != 1
                    || !complete_disclosed_tool(tool)
                {
                    continue;
                }
                let schema_sha256 = tool["schema_sha256"].as_str().expect("validated digest");
                disclosures.insert(chatos_local_agent_protocol::LocalRunCapabilityDisclosure {
                    plugin_option: plugin_option.to_string(),
                    tool_option: tool_option.to_string(),
                    provider_id: provider_id.to_string(),
                    schema_sha256: schema_sha256.to_ascii_lowercase(),
                });
            }
        }
        Ok(disclosures.into_iter().collect())
    }

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
                let mut missing = Vec::new();
                for skill_ref in required {
                    if !self.skill_is_activated(run_id, skill_ref).await? {
                        missing.push(skill_ref.to_string());
                    }
                }
                if missing.is_empty() {
                    Ok(Ok(()))
                } else {
                    Ok(Err(LocalToolSkillGateDenial {
                        error: "skill_activation_required".into(),
                        tool_name: tool_name.to_string(),
                        binding_id: binding.binding_id.clone(),
                        next_action: Some(serde_json::json!({
                            "tool_name": LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
                            "arguments": {"skill_ref": missing[0]},
                            "then": "Activate remaining required_skill_refs, then retry the original invocation."
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
                Some(serde_json::json!({
                    "tool_name": LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
                    "arguments": {"skill_ref": required_skill_refs[0]},
                    "then": "Activate remaining required_skill_refs, then retry the original invocation."
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
        for skill_ref in required {
            if !self.skill_is_activated(&run.run_id, &skill_ref).await? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::*;
    use serde_json::{json, Value};
    use std::sync::Arc;

    fn request(command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: uuid::Uuid::new_v4().to_string(),
            command,
        }
    }

    async fn check(
        runtime: &LocalAgentRuntime,
        arguments: Value,
    ) -> Result<(), LocalToolSkillGateDenial> {
        runtime
            .authorize_tool_skill_invocation("run-1", "capability_invoke", &arguments)
            .await
            .unwrap()
    }

    async fn claimed(
        runtime: &LocalAgentRuntime,
        name: &str,
        arguments: Value,
    ) -> LocalAgentToolClaim {
        let HostResult::Claim { claim: Some(claim) } = runtime
            .try_handle(request(HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".into(),
                worker_id: "model".into(),
                lease_duration_ms: 10_000,
            })))
            .await
            .expect("claim run")
        else {
            panic!("run claim")
        };
        runtime
            .try_handle(request(HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".into(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: uuid::Uuid::new_v4().to_string(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: uuid::Uuid::new_v4().to_string(),
                        tool_name: name.into(),
                        arguments,
                        side_effecting: false,
                        requires_approval: false,
                    }],
                    checkpoint: json!({}),
                },
            })))
            .await
            .expect("queue fact");
        let HostResult::ToolClaim { claim: Some(claim) } = runtime
            .try_handle(request(HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".into(),
                worker_id: "native".into(),
                lease_duration_ms: 10_000,
                include_tool_names: Some(vec![name.into()]),
                exclude_tool_names: vec![],
                execution_route: None,
            })))
            .await
            .expect("claim fact")
        else {
            panic!("tool claim")
        };
        claim
    }

    async fn fact(runtime: &LocalAgentRuntime, name: &str, arguments: Value, output: Value) {
        let claim = claimed(runtime, name, arguments).await;
        assert!(claim.preflight_outcome.is_none());
        runtime
            .try_handle(request(HostCommand::CommitTool(CommitToolCommand {
                owner_user_id: "user-1".into(),
                invocation_id: claim.invocation.invocation_id,
                claim_token: claim.claim_token,
                expected_version: claim.invocation.version,
                outcome: LocalAgentToolOutcome::Succeeded { output },
            })))
            .await
            .expect("commit fact");
    }

    #[tokio::test]
    async fn discovered_plugin_gate_uses_durable_success_and_survives_restart() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::new(storage.clone());
        runtime.initialize("user-1").await.expect("initialize");
        let bundle = LocalSkillBundleSnapshot {
            schema_version: 2,
            bundle_revision: "bundle-2".into(),
            documents: vec![LocalSkillDocumentSnapshot {
                skill_ref: "product-skill:discovery".into(),
                name: "discovery".into(),
                role: "router".into(),
                description: "Discover".into(),
                instructions: "Describe before invocation".into(),
                instructions_sha256: "a".repeat(64),
                resources: vec![],
            }],
            tool_skill_bindings: vec![],
        };
        runtime
            .try_handle(request(HostCommand::PutCapabilityPolicySnapshot(
                PutCapabilityPolicySnapshotCommand {
                    snapshot: LocalCapabilityPolicySnapshot {
                        owner_user_id: "user-1".into(),
                        profile_key: LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY.into(),
                        capability_policy_revision: "policy".into(),
                        instructions: Some(serde_json::to_string(&bundle).unwrap()),
                        prefixed_input_items: vec![],
                        tools: vec![],
                    },
                },
            )))
            .await
            .expect("bundle");
        runtime
            .try_handle(request(HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-1".into(),
                owner_user_id: "user-1".into(),
                owner_entity_type: "conversation".into(),
                owner_entity_id: "conversation".into(),
                profile_key: "task_execution".into(),
                model_config_ref: "model".into(),
                model_config_revision: "model-rev".into(),
                capability_policy_revision: "policy".into(),
                input: json!({}),
                max_iterations: 30,
            })))
            .await
            .expect("run");
        let invoke = json!({"plugin_option":"plugin_1", "tool_option":"tool_1", "arguments":{"format":"pdf"}});
        let denied = check(&runtime, invoke.clone()).await.unwrap_err();
        assert_eq!(
            denied.next_action.unwrap()["arguments"],
            json!({"plugin_option":"plugin_1", "tool_option":"tool_1"})
        );
        assert_eq!(
            check(&runtime, invoke.clone()).await.unwrap_err().error,
            "skill_binding_required"
        );
        let router = "plugin-skill:plugin_1:router";
        let pdf = "plugin-skill:plugin_1:pdf";
        let word = "plugin-skill:plugin_1:word";
        let description = json!({"binding_schema_version":1, "disclosure_version":2, "view":"tool", "provider_id":"plugin:document", "plugin_option":"plugin_1",
            "tools":[{"tool_option":"tool_1", "invocation_mode":"capability_invoke", "required_skill_refs":[router,pdf,word],
                "input_schema":{"type":"object"}, "schema_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "skill_gate":{"all_of":[router],"select_by_argument":{"pointer":"/format","map":{"pdf":pdf,"word":word}}}}]});
        let index = json!({"binding_schema_version":1,"disclosure_version":2,"view":"catalog",
            "provider_id":"plugin:document","plugin_option":"plugin_1","tools":[{"tool_option":"tool_1"}]});
        fact(
            &runtime,
            "capability_describe",
            json!({"plugin_option":"plugin_1"}),
            json!({"content":index,"is_error":false}),
        )
        .await;
        assert!(runtime
            .capability_disclosures_for_run("user-1", "run-1")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            check(&runtime, invoke.clone()).await.unwrap_err().error,
            "skill_binding_required",
            "an index is not a callable tool definition"
        );
        let mut old_description = description.clone();
        old_description.as_object_mut().unwrap().remove("view");
        old_description
            .as_object_mut()
            .unwrap()
            .remove("disclosure_version");
        fact(
            &runtime,
            "capability_describe",
            json!({"plugin_option":"plugin_1"}),
            json!({"content":old_description,"is_error":false}),
        )
        .await;
        assert_eq!(
            check(&runtime, invoke.clone()).await.unwrap_err().error,
            "skill_binding_required",
            "old full-catalog descriptions must not authorize a tool"
        );
        fact(
            &runtime,
            "capability_describe",
            json!({"plugin_option":"plugin_1"}),
            json!({"content":description,"is_error":true}),
        )
        .await;
        assert_eq!(
            check(&runtime, invoke.clone()).await.unwrap_err().error,
            "skill_binding_required"
        );
        fact(
            &runtime,
            "capability_describe",
            json!({"plugin_option":"plugin_1","tool_option":"tool_1"}),
            json!({"content":description,"is_error":false}),
        )
        .await;
        assert_eq!(
            runtime
                .capability_disclosures_for_run("user-1", "run-1")
                .await
                .unwrap(),
            vec![LocalRunCapabilityDisclosure {
                plugin_option: "plugin_1".into(),
                tool_option: "tool_1".into(),
                provider_id: "plugin:document".into(),
                schema_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .into(),
            }]
        );
        let mut catalog_description = description.clone();
        catalog_description["view"] = json!("catalog");
        catalog_description["tools"][0]["tool_option"] = json!("tool_2");
        catalog_description["tools"][0]["schema_sha256"] =
            json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        fact(
            &runtime,
            "capability_describe",
            json!({"plugin_option":"plugin_1"}),
            json!({"content":catalog_description,"is_error":false}),
        )
        .await;
        let batched = runtime
            .capability_disclosures_for_run("user-1", "run-1")
            .await
            .unwrap();
        assert_eq!(batched.len(), 2);
        assert_eq!(
            batched
                .iter()
                .map(|item| item.tool_option.as_str())
                .collect::<Vec<_>>(),
            vec!["tool_1", "tool_2"]
        );
        assert_ne!(
            check(
                &runtime,
                json!({"plugin_option":"plugin_1", "tool_option":"tool_2", "arguments":{"format":"pdf"}})
            )
            .await
            .unwrap_err()
            .error,
            "skill_binding_required",
            "a complete catalog entry must be a durable authorization fact"
        );
        assert_eq!(
            check(&runtime, invoke.clone())
                .await
                .unwrap_err()
                .required_skill_refs,
            vec![pdf, router]
        );
        fact(
            &runtime,
            "agent_skill_activate",
            json!({"skill_ref":router}),
            json!({"is_error":false,"content":{}}),
        )
        .await;
        let denied = check(&runtime, invoke.clone()).await.unwrap_err();
        assert_eq!(
            denied.next_action.unwrap()["arguments"],
            json!({"skill_ref":pdf})
        );
        fact(
            &runtime,
            "agent_skill_activate",
            json!({"skill_ref":pdf}),
            json!({"is_error":true,"content":"rejected"}),
        )
        .await;
        assert_eq!(
            check(&runtime, invoke.clone())
                .await
                .unwrap_err()
                .required_skill_refs,
            vec![pdf]
        );
        fact(
            &runtime,
            "agent_skill_activate",
            json!({"skill_ref":pdf}),
            json!({"is_error":false,"content":{}}),
        )
        .await;
        let restarted = LocalAgentRuntime::new(storage);
        assert!(check(&restarted, invoke.clone()).await.is_ok());
        let mut second = description.clone();
        second["disclosure_version"] = json!(2);
        second["view"] = json!("tool");
        second["tools"][0]["tool_option"] = json!("tool_2");
        fact(
            &restarted,
            "capability_describe",
            json!({"plugin_option":"plugin_1","tool_option":"tool_2"}),
            json!({"content":second,"is_error":false}),
        )
        .await;
        fact(
            &restarted,
            "capability_describe",
            json!({"plugin_option":"plugin_1","offset":8}),
            json!({"content":index,"is_error":false}),
        )
        .await;
        assert!(
            check(&restarted, invoke.clone()).await.is_ok(),
            "describing another tool or index page must retain the first tool's durable gate"
        );
        assert_eq!(check(&restarted, json!({"plugin_option":"plugin_1","tool_option":"tool_1","arguments":{"format":"word"}}))
            .await.unwrap_err().required_skill_refs, vec![word]);
        for arguments in [json!({"format":"unknown"}), json!({}), json!({"format":1})] {
            assert_eq!(
                check(
                    &restarted,
                    json!({"plugin_option":"plugin_1","tool_option":"tool_1","arguments":arguments})
                )
                .await
                .unwrap_err()
                .error,
                "skill_binding_invalid"
            );
        }
        assert_eq!(
            check(
                &restarted,
                json!({"plugin_option":"plugin_2","tool_option":"tool_1","arguments":{}})
            )
            .await
            .unwrap_err()
            .error,
            "skill_binding_required"
        );
        let mut direct = description.clone();
        direct["tools"][0]["invocation_mode"] = json!("direct_task_tool");
        fact(
            &restarted,
            "capability_describe",
            json!({"plugin_option":"plugin_1"}),
            json!({"content":direct,"is_error":false}),
        )
        .await;
        assert_eq!(
            check(&restarted, invoke).await.unwrap_err().error,
            "skill_binding_invalid"
        );
        assert_eq!(
            restarted
                .authorize_tool_skill_for_run("run-1", "external_mcp__missing__read")
                .await
                .unwrap()
                .unwrap_err()
                .error,
            "skill_binding_required"
        );
        let claim = claimed(
            &restarted,
            "capability_invoke",
            json!({"plugin_option":"plugin_2","tool_option":"tool_1","arguments":{}}),
        )
        .await;
        let Some(LocalAgentToolOutcome::Failed { error, .. }) = claim.preflight_outcome else {
            panic!("IPC claim must carry the shared dynamic gate denial");
        };
        assert_eq!(error, "skill_binding_required");
    }
}
