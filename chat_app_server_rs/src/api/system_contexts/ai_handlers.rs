// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::user_scope::ensure_and_set_user_id;
use crate::modules::platform_admin::system_context_ai::{
    evaluate_system_context_draft_usecase, generate_system_context_draft_usecase,
    optimize_system_context_draft_usecase, EvaluateSystemContextDraftUsecaseInput,
    GenerateSystemContextDraftUsecaseInput, OptimizeSystemContextDraftUsecaseInput,
};

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
    match generate_system_context_draft_usecase(GenerateSystemContextDraftUsecaseInput {
        user_id: req.user_id,
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
    match optimize_system_context_draft_usecase(OptimizeSystemContextDraftUsecaseInput {
        user_id: req.user_id,
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
    match evaluate_system_context_draft_usecase(EvaluateSystemContextDraftUsecaseInput {
        user_id: Some(auth.user_id),
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
