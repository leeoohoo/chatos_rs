use super::*;

pub(super) fn validate_asset_run_context(
    profile_key: &str,
    input: &Value,
    binding: &AgentAssetToolBinding,
) -> Result<(), String> {
    let identity = input
        .get("agent_execution_context")
        .and_then(Value::as_object)
        .ok_or_else(|| "The team asset Run context is invalid.".to_owned())?;
    if identity.get("schema_version").and_then(Value::as_u64) != Some(2)
        || identity.get("room_id").and_then(Value::as_str) != Some(&binding.team_id)
        || identity.get("agent_id").and_then(Value::as_str) != Some(&binding.agent_id)
        || identity.get("is_project_manager").and_then(Value::as_bool)
            != Some(binding.is_project_manager)
    {
        return Err("The team asset Run context is invalid.".into());
    }
    match binding.todo_id.as_deref() {
        Some(todo_id) => {
            let executor = input
                .get("team_executor_context")
                .and_then(Value::as_object)
                .ok_or_else(|| "The team asset executor context is invalid.".to_owned())?;
            if profile_key != "task_execution"
                || executor.get("delivery_lane").and_then(Value::as_str) != Some("executor")
                || executor.get("todo_id").and_then(Value::as_str) != Some(todo_id)
                || executor.get("team_id").and_then(Value::as_str) != Some(&binding.team_id)
                || executor.get("agent_id").and_then(Value::as_str) != Some(&binding.agent_id)
            {
                return Err("The team asset executor context is invalid.".into());
            }
        }
        None if profile_key == "main_chat" && input.get("team_executor_context").is_none() => {}
        None => return Err("The team asset manager context is invalid.".into()),
    }
    Ok(())
}

pub(super) fn validate_communication_run_context(
    profile_key: &str,
    input: &Value,
    binding: &AgentCommunicationToolBinding,
) -> Result<(), String> {
    let identity = input
        .get("agent_execution_context")
        .and_then(Value::as_object)
        .ok_or_else(|| "The Agent communication Run context is invalid.".to_owned())?;
    let schema_matches = match binding.conversation_kind.as_str() {
        "project_team" => {
            identity.get("schema_version").and_then(Value::as_u64) == Some(2)
                && identity.get("is_project_manager").and_then(Value::as_bool)
                    == Some(binding.is_project_manager)
        }
        "agent_agent_direct" => {
            identity.get("schema_version").and_then(Value::as_u64) == Some(3)
                && identity.get("conversation_kind").and_then(Value::as_str)
                    == Some("agent_agent_direct")
        }
        "human_agent_direct" => {
            (identity.get("schema_version").and_then(Value::as_u64) == Some(1)
                && identity.get("conversation_kind").is_none())
                || (identity.get("schema_version").and_then(Value::as_u64) == Some(3)
                    && identity.get("conversation_kind").and_then(Value::as_str)
                        == Some("human_agent_direct"))
        }
        _ => false,
    };
    if profile_key != "main_chat"
        || input.get("team_executor_context").is_some()
        || !schema_matches
        || identity.get("room_id").and_then(Value::as_str) != Some(&binding.room_id)
        || identity.get("agent_id").and_then(Value::as_str) != Some(&binding.agent_id)
    {
        return Err("The Agent communication Run context is invalid.".into());
    }
    Ok(())
}

pub(super) fn asset_summary(
    owner_user_id: &str,
    run_id: &str,
    record: &AgentAssetToolRecord,
) -> Value {
    json!({
        "asset_ref": asset_reference(owner_user_id, run_id, record),
        "category": record.category,
        "title": record.title,
        "revision": record.revision,
    })
}

pub(super) fn asset_reference(
    owner_user_id: &str,
    run_id: &str,
    record: &AgentAssetToolRecord,
) -> String {
    let digest = Sha256::digest(
        format!(
            "chatos-team-asset-ref-v1\n{owner_user_id}\n{run_id}\n{}\n{}",
            record.asset_id, record.revision
        )
        .as_bytes(),
    );
    format!("asset_ref_{digest:x}")
}

pub(super) fn team_asset_review_reference(
    owner_user_id: &str,
    run_id: &str,
    review: &AgentTeamAssetReview,
) -> String {
    let digest = Sha256::digest(
        format!(
            "chatos-team-asset-review-ref-v1\n{owner_user_id}\n{run_id}\n{}\n{}",
            review.team_id, review.id
        )
        .as_bytes(),
    );
    format!("asset_review_ref_{digest:x}")
}

pub(super) fn team_asset_review_output(
    owner_user_id: &str,
    run_id: &str,
    review: &AgentTeamAssetReview,
) -> Value {
    json!({
        "review_ref": team_asset_review_reference(owner_user_id, run_id, review),
        "team_ref": todo_planning_reference(owner_user_id, run_id, "team", &review.team_id),
        "source_todo_title": review.todo_title,
        "source_agent_name": review.source_agent_name,
        "category": review.suggestion.category,
        "title": review.suggestion.title,
        "markdown": review.suggestion.markdown,
        "rationale": review.suggestion.rationale,
        "status": review.status,
        "review_reason": review.review_reason,
        "created_at_unix_ms": review.created_at_unix_ms,
    })
}

pub(super) fn invalid_asset_arguments() -> String {
    "The team asset arguments are invalid.".into()
}
