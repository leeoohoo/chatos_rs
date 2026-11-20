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
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(false);
        };
        Ok(self
            .activated_skill_refs_for_run(&run.owner_user_id, run_id)
            .await?
            .iter()
            .any(|activated| activated == skill_ref))
    }

    #[doc(hidden)]
    pub async fn activated_skill_refs_for_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Vec<String>, LocalAgentRuntimeError> {
        let invocations = self
            .store
            .successful_tool_invocations_for_run(
                owner_user_id,
                run_id,
                LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
            )
            .await?;
        let mut skill_refs = invocations
            .into_iter()
            .flat_map(|invocation| {
                let single = invocation
                    .arguments
                    .get("skill_ref")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .into_iter();
                let batch = invocation
                    .arguments
                    .get("skill_refs")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string);
                single.chain(batch).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        skill_refs.sort();
        skill_refs.dedup();
        Ok(skill_refs)
    }
}
