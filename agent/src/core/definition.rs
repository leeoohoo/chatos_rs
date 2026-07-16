// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::ModelRuntimeConfig;

use crate::AgentDescriptor;

pub trait AgentIdentity: Send + Sync {
    fn descriptor(&self) -> &'static AgentDescriptor;
}

pub trait SystemAgentDefinition: AgentIdentity {
    fn system_prompt(&self) -> &'static str;
    fn message_mode(&self) -> &'static str;
    fn message_source(&self) -> &'static str;
    fn context_overflow_trigger(&self) -> &'static str;

    fn default_temperature(&self) -> Option<f64> {
        None
    }

    fn default_max_output_tokens(&self) -> Option<i64> {
        None
    }

    fn configure_model(&self, mut model_config: ModelRuntimeConfig) -> ModelRuntimeConfig {
        model_config.instructions = Some(merge_system_instructions(
            model_config.instructions.as_deref(),
            self.system_prompt(),
        ));
        if model_config.temperature.is_none() {
            model_config.temperature = self.default_temperature();
        }
        if model_config.max_output_tokens.is_none() {
            model_config.max_output_tokens = self.default_max_output_tokens();
        }
        model_config
    }
}

pub fn merge_system_instructions(existing: Option<&str>, required: &str) -> String {
    existing
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}\n\n{required}"))
        .unwrap_or_else(|| required.to_string())
}

#[cfg(test)]
mod tests {
    use chatos_plugin_management_sdk::SystemAgentKey;

    use super::*;

    struct TestAgent;

    impl AgentIdentity for TestAgent {
        fn descriptor(&self) -> &'static AgentDescriptor {
            crate::agent_descriptor(SystemAgentKey::ProjectManagementAgent)
        }
    }

    impl SystemAgentDefinition for TestAgent {
        fn system_prompt(&self) -> &'static str {
            "required prompt"
        }

        fn message_mode(&self) -> &'static str {
            "test"
        }

        fn message_source(&self) -> &'static str {
            "test"
        }

        fn context_overflow_trigger(&self) -> &'static str {
            "test_overflow"
        }

        fn default_temperature(&self) -> Option<f64> {
            Some(0.1)
        }

        fn default_max_output_tokens(&self) -> Option<i64> {
            Some(800)
        }
    }

    #[test]
    fn configures_model_without_overwriting_explicit_values() {
        let base = ModelRuntimeConfig::openai_compatible(
            "http://localhost",
            "key",
            "model",
            "openai_compatible",
        )
        .with_instructions(Some("existing".to_string()))
        .with_temperature(Some(0.7));

        let configured = TestAgent.configure_model(base);

        assert_eq!(
            configured.instructions.as_deref(),
            Some("existing\n\nrequired prompt")
        );
        assert_eq!(configured.temperature, Some(0.7));
        assert_eq!(configured.max_output_tokens, Some(800));
    }
}
