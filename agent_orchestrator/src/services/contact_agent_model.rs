use serde_json::Value;

use crate::services::memory_server_client;

const CLONE_META_KEY: &str = "__agent_workspace_clone_meta";

pub async fn resolve_effective_contact_agent_model_config_id(
    contact_agent_id: &str,
) -> Result<Option<String>, String> {
    let Some(agent) =
        memory_server_client::get_memory_agent_runtime_context(contact_agent_id).await?
    else {
        return Ok(None);
    };

    if let Some(model_id) = normalize_optional_model_id(agent.model_config_id.clone()) {
        return Ok(Some(model_id));
    }

    let Some(source_agent_id) = extract_clone_source_agent_id(agent.project_policy.as_ref()) else {
        return Ok(None);
    };

    let source_agent = memory_server_client::get_memory_agent(source_agent_id.as_str()).await?;
    Ok(source_agent.and_then(|item| normalize_optional_model_id(item.model_config_id)))
}

pub fn normalize_optional_model_id(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn extract_clone_source_agent_id(project_policy: Option<&Value>) -> Option<String> {
    project_policy
        .and_then(|policy| {
            policy
                .as_object()
                .and_then(|map| {
                    map.iter().find_map(|(key, value)| {
                        if key == CLONE_META_KEY || key.ends_with("_clone_meta") {
                            Some(value)
                        } else {
                            None
                        }
                    })
                })
        })
        .and_then(|meta| {
            meta.get("source_agent_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::{extract_clone_source_agent_id, normalize_optional_model_id};
    use serde_json::json;

    #[test]
    fn normalizes_optional_model_id() {
        assert_eq!(normalize_optional_model_id(None), None);
        assert_eq!(normalize_optional_model_id(Some("   ".to_string())), None);
        assert_eq!(
            normalize_optional_model_id(Some(" model-1 ".to_string())),
            Some("model-1".to_string())
        );
    }

    #[test]
    fn extracts_clone_source_agent_id() {
        let policy = json!({
            "__agent_workspace_clone_meta": {
                "source_agent_id": " admin-agent "
            }
        });
        assert_eq!(
            extract_clone_source_agent_id(Some(&policy)),
            Some("admin-agent".to_string())
        );
        let legacy_policy = json!({
            "__legacy_clone_meta": {
                "source_agent_id": " legacy-admin-agent "
            }
        });
        assert_eq!(
            extract_clone_source_agent_id(Some(&legacy_policy)),
            Some("legacy-admin-agent".to_string())
        );
        assert_eq!(extract_clone_source_agent_id(None), None);
    }
}
