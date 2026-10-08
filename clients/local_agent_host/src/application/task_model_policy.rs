// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::LocalAgentRunRecord;
use chatos_local_agent_runtime::LocalAgentRuntime;

pub(super) async fn resolve_task_model(
    runtime: &LocalAgentRuntime,
    parent: &LocalAgentRunRecord,
    requested: Option<&str>,
    allowed_model_config_ids: Option<&[String]>,
) -> Result<(String, String), String> {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    let requested = match allowed_model_config_ids {
        None => requested,
        Some([]) => return Err("no model configuration is enabled for local Tasks".to_string()),
        Some(allowed) => {
            if let Some(requested) = requested {
                if !allowed.iter().any(|candidate| candidate == requested) {
                    return Err(format!(
                        "model configuration is not enabled for local Tasks: {requested}"
                    ));
                }
                Some(requested)
            } else if allowed
                .iter()
                .any(|candidate| candidate == &parent.model_config_ref)
            {
                Some(parent.model_config_ref.as_str())
            } else {
                allowed.first().map(String::as_str)
            }
        }
    };
    if requested.is_none() || requested == Some(parent.model_config_ref.as_str()) {
        return Ok((
            parent.model_config_ref.clone(),
            parent.model_config_revision.clone(),
        ));
    }
    let requested = requested.unwrap_or_default();
    let snapshot = runtime
        .latest_model_config_for_task(&parent.owner_user_id, requested)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("model config not found: {requested}"))?;
    Ok((snapshot.model_config_ref, snapshot.model_config_revision))
}
