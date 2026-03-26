use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::core::auth::AuthUser;
use crate::core::system_context_access::{
    ensure_owned_system_context, map_system_context_access_error,
};
use crate::core::user_scope::resolve_user_id;
use crate::models::system_context::SystemContext;
use crate::repositories::system_contexts as ctx_repo;

use super::contracts::{ActivateContextRequest, SystemContextRequest, UserQuery};
use super::support::system_context_value;

pub(super) async fn list_system_contexts(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
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
        out.push(system_context_value(&ctx, Some(app_ids)));
    }
    (StatusCode::OK, Json(Value::Array(out)))
}

pub(super) async fn get_active_system_context(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
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
                "context": system_context_value(&ctx, Some(app_ids))
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "content": "", "context": Value::Null })),
    )
}

pub(super) async fn create_system_context(
    auth: AuthUser,
    Json(req): Json<SystemContextRequest>,
) -> (StatusCode, Json<Value>) {
    let Some(name) = req.name else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建系统上下文失败"})),
        );
    };
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
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

pub(super) async fn update_system_context(
    auth: AuthUser,
    Path(context_id): Path<String>,
    Json(req): Json<SystemContextRequest>,
) -> (StatusCode, Json<Value>) {
    let mut ctx = match ensure_owned_system_context(&context_id, &auth).await {
        Ok(ctx) => ctx,
        Err(err) => return map_system_context_access_error(err),
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

pub(super) async fn delete_system_context(
    auth: AuthUser,
    Path(context_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_system_context(&context_id, &auth).await {
        return map_system_context_access_error(err);
    }
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

pub(super) async fn activate_system_context(
    auth: AuthUser,
    Path(context_id): Path<String>,
    Json(req): Json<ActivateContextRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    if let Err(err) = ensure_owned_system_context(&context_id, &auth).await {
        return map_system_context_access_error(err);
    }
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
