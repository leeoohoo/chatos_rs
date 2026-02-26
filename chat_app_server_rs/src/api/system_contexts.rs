use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::system_context::SystemContext;
use crate::repositories::system_contexts as ctx_repo;
use crate::services::system_context_ai::{
    EvaluateDraftInput, GenerateDraftInput, OptimizeDraftInput, SystemContextAiError,
};

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SystemContextRequest {
    name: Option<String>,
    content: Option<String>,
    user_id: Option<String>,
    is_active: Option<bool>,
    app_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SystemContextAiGenerateRequest {
    user_id: Option<String>,
    scene: Option<String>,
    style: Option<String>,
    language: Option<String>,
    output_format: Option<String>,
    constraints: Option<Vec<String>>,
    forbidden: Option<Vec<String>>,
    candidate_count: Option<usize>,
    ai_model_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SystemContextAiOptimizeRequest {
    user_id: Option<String>,
    content: Option<String>,
    goal: Option<String>,
    keep_intent: Option<bool>,
    ai_model_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SystemContextAiEvaluateRequest {
    content: Option<String>,
    ai_model_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ActivateContextRequest {
    user_id: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/system-contexts",
            get(list_system_contexts).post(create_system_context),
        )
        .route(
            "/api/system-contexts/:context_id",
            put(update_system_context).delete(delete_system_context),
        )
        .route(
            "/api/system-contexts/:context_id/activate",
            post(activate_system_context),
        )
        .route(
            "/api/system-contexts/ai/generate",
            post(generate_system_context_draft),
        )
        .route(
            "/api/system-contexts/ai/optimize",
            post(optimize_system_context_draft),
        )
        .route(
            "/api/system-contexts/ai/evaluate",
            post(evaluate_system_context_draft),
        )
        .route("/api/system-context/active", get(get_active_system_context))
}

async fn list_system_contexts(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match query.user_id {
        Some(u) => u,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "user_id 为必填参数"})),
            )
        }
    };
    let contexts = match ctx_repo::list_system_contexts(&user_id).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取系统上下文失败", "detail": err})),
            )
        }
    };
    let mut out = Vec::new();
    for ctx in contexts {
        let app_ids = match ctx_repo::get_app_ids_for_system_context(&ctx.id).await {
            Ok(ids) => ids,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "获取系统上下文失败", "detail": err})),
                )
            }
        };
        out.push(json!({
            "id": ctx.id,
            "name": ctx.name,
            "content": ctx.content,
            "user_id": ctx.user_id,
            "is_active": ctx.is_active,
            "created_at": ctx.created_at,
            "updated_at": ctx.updated_at,
            "app_ids": app_ids
        }));
    }
    (StatusCode::OK, Json(Value::Array(out)))
}

async fn get_active_system_context(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match query.user_id {
        Some(u) => u,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "user_id 为必填参数"})),
            )
        }
    };
    let ctx = match ctx_repo::get_active_system_context(&user_id).await {
        Ok(ctx) => ctx,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取活跃系统上下文失败", "detail": err})),
            )
        }
    };
    if let Some(ctx) = ctx {
        let app_ids = match ctx_repo::get_app_ids_for_system_context(&ctx.id).await {
            Ok(ids) => ids,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "获取活跃系统上下文失败", "detail": err})),
                )
            }
        };
        return (
            StatusCode::OK,
            Json(json!({
                "content": ctx.content.clone().unwrap_or_default(),
                "context": {
                    "id": ctx.id,
                    "name": ctx.name,
                    "content": ctx.content,
                    "user_id": ctx.user_id,
                    "is_active": ctx.is_active,
                    "created_at": ctx.created_at,
                    "updated_at": ctx.updated_at,
                    "app_ids": app_ids
                }
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "content": "", "context": Value::Null })),
    )
}

async fn create_system_context(Json(req): Json<SystemContextRequest>) -> (StatusCode, Json<Value>) {
    let Some(name) = req.name else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建系统上下文失败"})),
        );
    };
    let Some(user_id) = req.user_id else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建系统上下文失败"})),
        );
    };
    let id = Uuid::new_v4().to_string();
    let ctx = SystemContext {
        id: id.clone(),
        name,
        content: req.content,
        user_id,
        is_active: req.is_active.unwrap_or(false),
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
    };
    if let Err(err) = ctx_repo::create_system_context(&ctx).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建系统上下文失败", "detail": err})),
        );
    }
    if let Some(app_ids) = req.app_ids.clone() {
        if let Err(err) = ctx_repo::set_app_ids_for_system_context(&id, &app_ids).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败", "detail": err})),
            );
        }
    }
    let ctx = match ctx_repo::get_system_context_by_id(&id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败", "detail": err})),
            )
        }
    };
    let app_ids = match ctx_repo::get_app_ids_for_system_context(&id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建系统上下文失败", "detail": err})),
            )
        }
    };
    let obj = system_context_value(&ctx, Some(app_ids));
    (StatusCode::CREATED, Json(obj))
}

async fn update_system_context(
    Path(context_id): Path<String>,
    Json(req): Json<SystemContextRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ctx_repo::get_system_context_by_id(&context_id).await {
        Ok(ctx) => ctx,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            )
        }
    };
    let Some(mut ctx) = existing else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新系统上下文失败"})),
        );
    };
    if let Some(v) = req.name {
        ctx.name = v;
    }
    if let Some(v) = req.content {
        ctx.content = Some(v);
    }
    if let Some(v) = req.is_active {
        ctx.is_active = v;
    }
    if let Err(err) = ctx_repo::update_system_context(&context_id, &ctx).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新系统上下文失败", "detail": err})),
        );
    }
    if let Some(app_ids) = req.app_ids {
        if let Err(err) = ctx_repo::set_app_ids_for_system_context(&context_id, &app_ids).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            );
        }
    }
    let ctx = match ctx_repo::get_system_context_by_id(&context_id).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            )
        }
    };
    let app_ids = match ctx_repo::get_app_ids_for_system_context(&context_id).await {
        Ok(ids) => ids,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新系统上下文失败", "detail": err})),
            )
        }
    };
    let obj = system_context_value(&ctx, Some(app_ids));
    (StatusCode::OK, Json(obj))
}

async fn delete_system_context(Path(context_id): Path<String>) -> (StatusCode, Json<Value>) {
    if let Err(err) = ctx_repo::delete_system_context(&context_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除系统上下文失败", "detail": err})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "message": "系统上下文删除成功" })),
    )
}

async fn activate_system_context(
    Path(context_id): Path<String>,
    Json(req): Json<ActivateContextRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match req.user_id {
        Some(u) => u,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "激活系统上下文失败"})),
            )
        }
    };
    if let Err(err) = ctx_repo::activate_system_context(&context_id, &user_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "激活系统上下文失败", "detail": err})),
        );
    }
    let list = match ctx_repo::list_system_contexts(&user_id).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "激活系统上下文失败", "detail": err})),
            )
        }
    };
    let activated = list.into_iter().find(|c| c.id == context_id);
    let out = activated
        .map(|ctx| system_context_value(&ctx, None))
        .unwrap_or(Value::Null);
    (StatusCode::OK, Json(out))
}

async fn generate_system_context_draft(
    Json(req): Json<SystemContextAiGenerateRequest>,
) -> (StatusCode, Json<Value>) {
    match crate::services::system_context_ai::generate_draft(GenerateDraftInput {
        user_id: req.user_id,
        scene: req.scene,
        style: req.style,
        language: req.language,
        output_format: req.output_format,
        constraints: req.constraints,
        forbidden: req.forbidden,
        candidate_count: req.candidate_count,
        ai_model_config: req.ai_model_config,
    })
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => map_system_context_ai_error(err),
    }
}

async fn optimize_system_context_draft(
    Json(req): Json<SystemContextAiOptimizeRequest>,
) -> (StatusCode, Json<Value>) {
    match crate::services::system_context_ai::optimize_draft(OptimizeDraftInput {
        user_id: req.user_id,
        content: req.content,
        goal: req.goal,
        keep_intent: req.keep_intent,
        ai_model_config: req.ai_model_config,
    })
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => map_system_context_ai_error(err),
    }
}

async fn evaluate_system_context_draft(
    Json(req): Json<SystemContextAiEvaluateRequest>,
) -> (StatusCode, Json<Value>) {
    match crate::services::system_context_ai::evaluate_draft(EvaluateDraftInput {
        content: req.content,
        ai_model_config: req.ai_model_config,
    })
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => map_system_context_ai_error(err),
    }
}

fn map_system_context_ai_error(err: SystemContextAiError) -> (StatusCode, Json<Value>) {
    match err {
        SystemContextAiError::BadRequest { message } => {
            (StatusCode::BAD_REQUEST, Json(json!({"error": message})))
        }
        SystemContextAiError::Upstream { message, raw } => {
            let mut body = json!({"error": message});
            if let Some(raw) = raw {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("raw".to_string(), Value::String(raw));
                }
            }
            (StatusCode::BAD_GATEWAY, Json(body))
        }
    }
}

fn system_context_value(ctx: &SystemContext, app_ids: Option<Vec<String>>) -> Value {
    let mut obj = json!({
        "id": ctx.id.clone(),
        "name": ctx.name.clone(),
        "content": ctx.content.clone(),
        "user_id": ctx.user_id.clone(),
        "is_active": ctx.is_active,
        "created_at": ctx.created_at.clone(),
        "updated_at": ctx.updated_at.clone(),
    });
    if let Some(ids) = app_ids {
        obj["app_ids"] = json!(ids);
    }
    obj
}
