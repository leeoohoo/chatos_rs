use crate::models::agent::Agent;
use crate::models::ai_model_config::AiModelConfig;
use crate::repositories::{agents, ai_model_configs, system_contexts};

#[derive(Debug, Clone)]
pub struct EnabledAgentModel {
    pub agent: Agent,
    pub model: AiModelConfig,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AgentModelLoadError {
    AgentUnavailable,
    ModelUnavailable,
    Repository(String),
}

pub async fn load_enabled_agent_model(
    agent_id: &str,
) -> Result<EnabledAgentModel, AgentModelLoadError> {
    let agent = agents::get_agent_by_id(agent_id)
        .await
        .map_err(AgentModelLoadError::Repository)?
        .ok_or(AgentModelLoadError::AgentUnavailable)?;

    if !agent.enabled {
        return Err(AgentModelLoadError::AgentUnavailable);
    }

    let model = ai_model_configs::get_ai_model_config_by_id(&agent.ai_model_config_id)
        .await
        .map_err(AgentModelLoadError::Repository)?
        .ok_or(AgentModelLoadError::ModelUnavailable)?;

    if !model.enabled {
        return Err(AgentModelLoadError::ModelUnavailable);
    }

    let system_prompt = load_system_prompt_from_active_context(agent.system_context_id.as_deref())
        .await
        .map_err(AgentModelLoadError::Repository)?;

    Ok(EnabledAgentModel {
        agent,
        model,
        system_prompt,
    })
}

async fn load_system_prompt_from_active_context(
    context_id: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(context_id) = context_id else {
        return Ok(None);
    };

    match system_contexts::get_system_context_by_id(context_id).await? {
        Some(context) if context.is_active => Ok(context.content),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::AgentModelLoadError;

    #[test]
    fn preserves_repository_error_message() {
        let err = AgentModelLoadError::Repository("oops".to_string());
        match err {
            AgentModelLoadError::Repository(message) => assert_eq!(message, "oops"),
            _ => panic!("expected repository error variant"),
        }
    }
}
