use serde_json::Value;

use crate::core::internal_context_locale::InternalContextLocale;
use crate::modules::conversation_runtime::user_context::load_runtime_user_context;
use crate::services::system_context_ai::{
    evaluate_draft, generate_draft, optimize_draft, EvaluateDraftInput, GenerateDraftInput,
    OptimizeDraftInput, PromptRuntimeOverrides, SystemContextAiError,
};

#[derive(Debug, Clone)]
pub struct GenerateSystemContextDraftUsecaseInput {
    pub user_id: Option<String>,
    pub scene: Option<String>,
    pub style: Option<String>,
    pub language: Option<String>,
    pub output_format: Option<String>,
    pub constraints: Option<Vec<String>>,
    pub forbidden: Option<Vec<String>>,
    pub candidate_count: Option<usize>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<PromptRuntimeOverrides>,
}

#[derive(Debug, Clone)]
pub struct OptimizeSystemContextDraftUsecaseInput {
    pub user_id: Option<String>,
    pub content: Option<String>,
    pub goal: Option<String>,
    pub keep_intent: Option<bool>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<PromptRuntimeOverrides>,
}

#[derive(Debug, Clone)]
pub struct EvaluateSystemContextDraftUsecaseInput {
    pub user_id: Option<String>,
    pub content: Option<String>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<PromptRuntimeOverrides>,
}

pub async fn generate_system_context_draft_usecase(
    input: GenerateSystemContextDraftUsecaseInput,
) -> Result<Value, SystemContextAiError> {
    let locale = resolve_user_locale(input.user_id.clone()).await;
    generate_draft(GenerateDraftInput {
        user_id: input.user_id,
        internal_context_locale: locale,
        scene: input.scene,
        style: input.style,
        language: input.language,
        output_format: input.output_format,
        constraints: input.constraints,
        forbidden: input.forbidden,
        candidate_count: input.candidate_count,
        model_config_id: input.model_config_id,
        ai_model_config: input.ai_model_config.filter(|value| !value.is_empty()),
    })
    .await
}

pub async fn optimize_system_context_draft_usecase(
    input: OptimizeSystemContextDraftUsecaseInput,
) -> Result<Value, SystemContextAiError> {
    let locale = resolve_user_locale(input.user_id.clone()).await;
    optimize_draft(OptimizeDraftInput {
        user_id: input.user_id,
        internal_context_locale: locale,
        content: input.content,
        goal: input.goal,
        keep_intent: input.keep_intent,
        model_config_id: input.model_config_id,
        ai_model_config: input.ai_model_config.filter(|value| !value.is_empty()),
    })
    .await
}

pub async fn evaluate_system_context_draft_usecase(
    input: EvaluateSystemContextDraftUsecaseInput,
) -> Result<Value, SystemContextAiError> {
    let locale = resolve_user_locale(input.user_id).await;
    evaluate_draft(EvaluateDraftInput {
        internal_context_locale: locale,
        content: input.content,
        model_config_id: input.model_config_id,
        ai_model_config: input.ai_model_config.filter(|value| !value.is_empty()),
    })
    .await
}

async fn resolve_user_locale(user_id: Option<String>) -> InternalContextLocale {
    load_runtime_user_context(user_id, "").await.locale
}
