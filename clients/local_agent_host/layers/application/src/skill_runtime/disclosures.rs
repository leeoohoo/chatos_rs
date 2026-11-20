impl LocalAgentRuntime {
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
                let Some(tool_name) = tool.get("name").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                disclosures.insert(chatos_local_agent_protocol::LocalRunCapabilityDisclosure {
                    plugin_option: plugin_option.to_string(),
                    tool_option: tool_option.to_string(),
                    tool_name: tool_name.to_string(),
                    provider_id: provider_id.to_string(),
                    schema_sha256: schema_sha256.to_ascii_lowercase(),
                });
            }
        }
        Ok(disclosures.into_iter().collect())
    }
}
