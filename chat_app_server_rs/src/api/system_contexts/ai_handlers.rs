use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::internal_context_locale::internal_context_locale_from_settings;
use crate::core::user_scope::ensure_and_set_user_id;
use crate::services::system_context_ai::{
    EvaluateDraftInput, GenerateDraftInput, OptimizeDraftInput,
};
use crate::services::user_settings::get_effective_user_settings;

use super::contracts::{
    SystemContextAiEvaluateRequest, SystemContextAiGenerateRequest, SystemContextAiOptimizeRequest,
};
use super::support::map_system_context_ai_error;

pub(super) async fn generate_system_context_draft(
    auth: AuthUser,
    Json(mut req): Json<SystemContextAiGenerateRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_and_set_user_id(&mut req.user_id, &auth) {
        return err;
    }
    let effective_settings = get_effective_user_settings(req.user_id.clone())
        .await
        .unwrap_or_else(|_| serde_json::json!({}));
    let internal_context_locale = internal_context_locale_from_settings(&effective_settings);
    match crate::services::system_context_ai::generate_draft(GenerateDraftInput {
        user_id: req.user_id,
        internal_context_locale,
        scene: req.scene,
        style: req.style,
        language: req.language,
        output_format: req.output_format,
        constraints: req.constraints,
        forbidden: req.forbidden,
        candidate_count: req.candidate_count,
        model_config_id: req.model_config_id,
        ai_model_config: req
            .ai_model_config
            .map(|value| value.into_runtime_overrides())
            .filter(|value| !value.is_empty()),
    })
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => map_system_context_ai_error(err),
    }
}

pub(super) async fn optimize_system_context_draft(
    auth: AuthUser,
    Json(mut req): Json<SystemContextAiOptimizeRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_and_set_user_id(&mut req.user_id, &auth) {
        return err;
    }
    let effective_settings = get_effective_user_settings(req.user_id.clone())
        .await
        .unwrap_or_else(|_| serde_json::json!({}));
    let internal_context_locale = internal_context_locale_from_settings(&effective_settings);
    match crate::services::system_context_ai::optimize_draft(OptimizeDraftInput {
        user_id: req.user_id,
        internal_context_locale,
        content: req.content,
        goal: req.goal,
        keep_intent: req.keep_intent,
        model_config_id: req.model_config_id,
        ai_model_config: req
            .ai_model_config
            .map(|value| value.into_runtime_overrides())
            .filter(|value| !value.is_empty()),
    })
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => map_system_context_ai_error(err),
    }
}

pub(super) async fn evaluate_system_context_draft(
    auth: AuthUser,
    Json(req): Json<SystemContextAiEvaluateRequest>,
) -> (StatusCode, Json<Value>) {
    let effective_settings = get_effective_user_settings(Some(auth.user_id))
        .await
        .unwrap_or_else(|_| serde_json::json!({}));
    let internal_context_locale = internal_context_locale_from_settings(&effective_settings);
    match crate::services::system_context_ai::evaluate_draft(EvaluateDraftInput {
        internal_context_locale,
        content: req.content,
        model_config_id: req.model_config_id,
        ai_model_config: req
            .ai_model_config
            .map(|value| value.into_runtime_overrides())
            .filter(|value| !value.is_empty()),
    })
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => map_system_context_ai_error(err),
    }
}
