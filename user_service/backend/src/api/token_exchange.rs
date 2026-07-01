// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::{Extension, Json};

use crate::auth::{encode_agent_token, CurrentPrincipal};
use crate::models::{
    TaskRunnerTokenExchangeRequest, TaskRunnerTokenExchangeResponse, TokenExchangePrincipalSummary,
    PRINCIPAL_TYPE_AGENT_ACCOUNT,
};
use crate::state::AppState;

use super::{forbidden, internal_error, not_found, ApiResult};

pub async fn exchange_task_runner_token(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<TaskRunnerTokenExchangeRequest>,
) -> ApiResult<TaskRunnerTokenExchangeResponse> {
    let Some(agent) = state
        .store
        .find_agent_by_id(input.task_runner_agent_account_id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("agent account not found"));
    };
    if !principal.is_super_admin()
        && principal.user_id.as_deref() != Some(agent.owner_user_id.as_str())
    {
        return Err(forbidden(
            "cannot exchange token for another user's agent account",
        ));
    }
    if !agent.enabled {
        return Err(forbidden("agent account is disabled"));
    }

    let Some(owner) = state
        .store
        .find_user_by_id(agent.owner_user_id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("agent owner not found"));
    };
    if !owner.enabled {
        return Err(forbidden("agent owner is disabled"));
    }

    let token = encode_agent_token(&state.config, &agent, &owner).map_err(internal_error)?;

    Ok(Json(TaskRunnerTokenExchangeResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: state.config.task_runner_access_ttl_seconds,
        principal: TokenExchangePrincipalSummary {
            principal_type: PRINCIPAL_TYPE_AGENT_ACCOUNT.to_string(),
            agent_account_id: agent.id,
            owner_user_id: owner.id,
            owner_username: owner.username,
        },
    }))
}
