use super::*;

pub(super) async fn execute_team_asset_tool(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if !TEAM_ASSET_TOOL_NAMES.contains(&claim.invocation.tool_name.as_str()) {
        return Err("The requested team asset tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The team asset arguments are invalid.".to_owned())?;
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("The team asset Run context is invalid.".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != claim.invocation.run_id {
        return Err("The team asset Run context is invalid.".into());
    }
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run_id = run.run_id.clone();
    let (binding, records) = tauri::async_runtime::spawn_blocking(move || {
        store.load_agent_asset_tool_records(&owner, &run_id)
    })
    .await
    .map_err(|_| "The team asset lookup task exited unexpectedly.".to_owned())??;
    validate_asset_run_context(&run.profile_key, &run.input, &binding)?;
    let output = match claim.invocation.tool_name.as_str() {
        "team_asset_list" => {
            let target_team_id = if binding.todo_id.is_some() {
                binding.team_id.clone()
            } else if let Some(team_reference) = arguments.get("team_ref") {
                resolve_asset_team_reference(
                    app,
                    owner_user_id,
                    &run.run_id,
                    team_reference
                        .as_str()
                        .filter(|value| !value.is_empty() && value.chars().count() <= 600)
                        .ok_or_else(invalid_asset_arguments)?,
                )
                .await?
            } else {
                binding.team_id.clone()
            };
            let category = arguments
                .get("category")
                .map(|value| value.as_str().ok_or_else(invalid_asset_arguments))
                .transpose()?
                .filter(|value| !value.is_empty());
            let assets = records
                .iter()
                .filter(|record| {
                    record.team_id == target_team_id
                        && category.is_none_or(|value| record.category == value)
                })
                .map(|record| asset_summary(owner_user_id, &run.run_id, record))
                .collect::<Vec<_>>();
            json!({"assets": assets, "snapshot": binding.todo_id.is_some()})
        }
        "team_asset_get" => {
            let asset_ref = arguments
                .get("asset_ref")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= 160)
                .ok_or_else(invalid_asset_arguments)?;
            let record = records
                .iter()
                .find(|record| asset_reference(owner_user_id, &run.run_id, record) == asset_ref)
                .ok_or_else(|| {
                    "The team asset reference is unavailable for this Run.".to_owned()
                })?;
            json!({
                "asset_ref": asset_ref,
                "category": record.category,
                "title": record.title,
                "markdown": record.markdown,
                "revision": record.revision,
                "snapshot": binding.todo_id.is_some(),
            })
        }
        "team_asset_create" => {
            require_asset_manager_lane(&binding)?;
            let team_reference = required_asset_string(arguments, "team_ref", 600)?;
            let target_team_id =
                resolve_asset_team_reference(app, owner_user_id, &run.run_id, team_reference)
                    .await?;
            let category = required_asset_string(arguments, "category", 64)?.to_owned();
            let title = required_asset_string(arguments, "title", 240)?.to_owned();
            let markdown = required_asset_text(arguments, "markdown", 128_000)?.to_owned();
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = target_team_id;
            let agent_id = binding.agent_id.clone();
            let saved = tauri::async_runtime::spawn_blocking(move || {
                store.save_project_team_asset_as_manager(
                    &owner,
                    &team_id,
                    &agent_id,
                    None,
                    &category,
                    &title,
                    &markdown,
                    None,
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The team asset create task exited unexpectedly.".to_owned())??;
            saved_asset_output(owner_user_id, &run.run_id, saved)
        }
        "team_asset_update" => {
            require_asset_manager_lane(&binding)?;
            let record = referenced_asset(arguments, owner_user_id, &run.run_id, &records)?;
            let expected_revision = record.revision;
            let category = required_asset_string(arguments, "category", 64)?.to_owned();
            let title = required_asset_string(arguments, "title", 240)?.to_owned();
            let markdown = required_asset_text(arguments, "markdown", 128_000)?.to_owned();
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = record.team_id.clone();
            let agent_id = binding.agent_id.clone();
            let asset_id = record.asset_id.clone();
            let saved = tauri::async_runtime::spawn_blocking(move || {
                store.save_project_team_asset_as_manager(
                    &owner,
                    &team_id,
                    &agent_id,
                    Some(&asset_id),
                    &category,
                    &title,
                    &markdown,
                    Some(expected_revision),
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The team asset update task exited unexpectedly.".to_owned())??;
            saved_asset_output(owner_user_id, &run.run_id, saved)
        }
        "team_asset_archive" => {
            require_asset_manager_lane(&binding)?;
            let record = referenced_asset(arguments, owner_user_id, &run.run_id, &records)?;
            let expected_revision = record.revision;
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let team_id = record.team_id.clone();
            let agent_id = binding.agent_id.clone();
            let asset_id = record.asset_id.clone();
            let archived = tauri::async_runtime::spawn_blocking(move || {
                store.archive_project_team_asset_as_manager(
                    &owner,
                    &team_id,
                    &agent_id,
                    &asset_id,
                    expected_revision,
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The team asset archive task exited unexpectedly.".to_owned())??;
            saved_asset_output(owner_user_id, &run.run_id, archived)
        }
        "team_asset_review_list" => {
            require_asset_manager_lane(&binding)?;
            let target_team_id = arguments
                .get("team_ref")
                .map(|value| {
                    value
                        .as_str()
                        .filter(|reference| {
                            !reference.is_empty() && reference.chars().count() <= 600
                        })
                        .ok_or_else(invalid_asset_arguments)
                })
                .transpose()?;
            let target_team_id = if let Some(reference) = target_team_id {
                Some(
                    resolve_asset_team_reference(app, owner_user_id, &run.run_id, reference)
                        .await?,
                )
            } else {
                None
            };
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let agent_id = binding.agent_id.clone();
            let reviews = tauri::async_runtime::spawn_blocking(move || {
                store.list_team_asset_reviews(
                    &owner,
                    target_team_id.as_deref(),
                    Some(&agent_id),
                    "pending",
                    500,
                )
            })
            .await
            .map_err(|_| "The team asset review lookup task exited unexpectedly.".to_owned())??;
            let reviews = reviews
                .iter()
                .map(|review| team_asset_review_output(owner_user_id, &run.run_id, review))
                .collect::<Vec<_>>();
            json!({"reviews": reviews})
        }
        "team_asset_review_resolve" => {
            require_asset_manager_lane(&binding)?;
            let review_ref = required_asset_string(arguments, "review_ref", 600)?;
            let outcome = required_asset_string(arguments, "outcome", 16)?;
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let agent_id = binding.agent_id.clone();
            let pending = tauri::async_runtime::spawn_blocking(move || {
                store.list_team_asset_reviews(&owner, None, Some(&agent_id), "pending", 500)
            })
            .await
            .map_err(|_| "The team asset review lookup task exited unexpectedly.".to_owned())??;
            let review = pending
                .iter()
                .find(|review| {
                    team_asset_review_reference(owner_user_id, &run.run_id, review) == review_ref
                })
                .ok_or_else(|| {
                    "The team asset review reference is unavailable for this Run.".to_owned()
                })?;
            let supplied_reason = arguments
                .get("reason")
                .map(|_| required_asset_string(arguments, "reason", 4_000).map(str::to_owned))
                .transpose()?
                .unwrap_or_default();
            let (resolved_asset, reason) = match outcome {
                "applied" => {
                    let record = referenced_asset(arguments, owner_user_id, &run.run_id, &records)?;
                    (
                        Some((record.asset_id.clone(), record.revision)),
                        supplied_reason,
                    )
                }
                "rejected" => {
                    if arguments.contains_key("asset_ref") {
                        return Err(invalid_asset_arguments());
                    }
                    if supplied_reason.is_empty() {
                        return Err(invalid_asset_arguments());
                    }
                    (None, supplied_reason)
                }
                _ => return Err(invalid_asset_arguments()),
            };
            let store = app.state::<AgentConversationStore>().inner().clone();
            let owner = owner_user_id.to_owned();
            let review_id = review.id.clone();
            let reviewer_agent_id = binding.agent_id.clone();
            let outcome = outcome.to_owned();
            let resolved_asset_owned = resolved_asset.clone();
            let updated = tauri::async_runtime::spawn_blocking(move || {
                store.resolve_team_asset_review(
                    &owner,
                    &review_id,
                    &reviewer_agent_id,
                    &outcome,
                    resolved_asset_owned
                        .as_ref()
                        .map(|(asset_id, revision)| (asset_id.as_str(), *revision)),
                    &reason,
                    now_unix_ms(),
                )
            })
            .await
            .map_err(|_| "The team asset review update task exited unexpectedly.".to_owned())??;
            team_asset_review_output(owner_user_id, &run.run_id, &updated)
        }
        _ => return Err("The requested team asset tool is unavailable.".into()),
    };
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

pub(super) fn require_asset_manager_lane(binding: &AgentAssetToolBinding) -> Result<(), String> {
    if binding.todo_id.is_some() || !binding.is_project_manager {
        Err("Only the active project manager lane may change or review team assets.".into())
    } else {
        Ok(())
    }
}

fn referenced_asset<'a>(
    arguments: &Map<String, Value>,
    owner_user_id: &str,
    run_id: &str,
    records: &'a [AgentAssetToolRecord],
) -> Result<&'a AgentAssetToolRecord, String> {
    let asset_ref = required_asset_string(arguments, "asset_ref", 160)?;
    records
        .iter()
        .find(|record| asset_reference(owner_user_id, run_id, record) == asset_ref)
        .ok_or_else(|| "The team asset reference is unavailable for this Run.".to_owned())
}

async fn resolve_asset_team_reference(
    app: &AppHandle,
    owner_user_id: &str,
    run_id: &str,
    reference: &str,
) -> Result<String, String> {
    let store = app.state::<AgentConversationStore>().inner().clone();
    let owner = owner_user_id.to_owned();
    let run = run_id.to_owned();
    let reference = reference.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        store.resolve_agent_asset_team_reference(&owner, &run, &reference)
    })
    .await
    .map_err(|_| "The team reference lookup task exited unexpectedly.".to_owned())?
}

fn required_asset_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.chars().count() <= maximum
                && !value.contains('\0')
        })
        .ok_or_else(invalid_asset_arguments)
}

fn required_asset_text<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| value.chars().count() <= maximum && !value.contains('\0'))
        .ok_or_else(invalid_asset_arguments)
}

fn saved_asset_output(
    owner_user_id: &str,
    run_id: &str,
    asset: crate::agent_conversation::AgentTeamAsset,
) -> Value {
    let record = AgentAssetToolRecord {
        team_id: asset.team_id,
        asset_id: asset.id,
        category: asset.category,
        title: asset.title,
        markdown: asset.markdown,
        revision: asset.revision,
    };
    json!({
        "asset_ref": asset_reference(owner_user_id, run_id, &record),
        "category": record.category,
        "title": record.title,
        "revision": record.revision,
    })
}

pub(super) fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
