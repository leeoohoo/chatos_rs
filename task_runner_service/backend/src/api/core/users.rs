use super::*;

#[derive(Debug, Deserialize)]
struct UserServiceUserSummary {
    id: String,
    username: String,
    display_name: String,
    role: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
    last_login_at: Option<String>,
    agent_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UserServiceAgentAccount {
    id: String,
    username: String,
    display_name: String,
    owner_user_id: String,
    owner_username: String,
    owner_display_name: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
    last_login_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct UserServiceCreateUserRequest {
    username: String,
    display_name: Option<String>,
    password: String,
    role: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
struct UserServiceUpdateUserRequest {
    display_name: Option<String>,
    password: Option<String>,
    role: Option<String>,
    enabled: Option<bool>,
}

pub(in crate::api) async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserSummaryRecord>>, ApiError> {
    let token = current_access_token()?;
    let agents = request_user_service_json::<(), Vec<UserServiceAgentAccount>>(
        &state,
        reqwest::Method::GET,
        "/api/agent-accounts",
        Some(token.as_str()),
        None,
    )
    .await?;

    let mut rows = agents
        .into_iter()
        .map(user_service_agent_to_task_runner_row)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then(left.username.cmp(&right.username))
    });
    Ok(Json(rows))
}

pub(in crate::api) async fn create_user(
    State(state): State<AppState>,
    Json(input): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserSummaryRecord>), ApiError> {
    let token = current_access_token()?;
    let payload = UserServiceCreateUserRequest {
        username: input.username,
        display_name: input.display_name,
        password: input.password,
        role: input.role.map(task_runner_role_to_user_service_role),
        enabled: input.enabled,
    };
    let created = request_user_service_json(
        &state,
        reqwest::Method::POST,
        "/api/users",
        Some(token.as_str()),
        Some(&payload),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(user_service_user_to_task_runner_row(created)),
    ))
}

pub(in crate::api) async fn update_user(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateUserRequest>,
) -> Result<Json<UserSummaryRecord>, ApiError> {
    let token = current_access_token()?;
    let payload = UserServiceUpdateUserRequest {
        display_name: input.display_name,
        password: input.password,
        role: input.role.map(task_runner_role_to_user_service_role),
        enabled: input.enabled,
    };
    let path = format!("/api/users/{id}");
    let updated = request_user_service_json(
        &state,
        reqwest::Method::PATCH,
        path.as_str(),
        Some(token.as_str()),
        Some(&payload),
    )
    .await?;
    Ok(Json(user_service_user_to_task_runner_row(updated)))
}

pub(in crate::api) async fn delete_user(
    Path(_id): Path<String>,
    State(_state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    require_admin_user(&current_user)?;
    Err(ApiError::forbidden(
        "统一用户由 user_service 管理，Task Runner 不再直接删除用户",
    ))
}

fn current_access_token() -> Result<String, ApiError> {
    crate::auth::get_current_access_token()
        .ok_or_else(|| ApiError::unauthorized("缺少 user_service 访问令牌"))
}

async fn request_user_service_json<TBody, TResp>(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<TResp, ApiError>
where
    TBody: Serialize + ?Sized,
    TResp: serde::de::DeserializeOwned,
{
    let response = request_user_service(state, method, path, access_token, body).await?;
    response
        .json::<TResp>()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("parse user_service response failed: {err}")))
}

async fn request_user_service<TBody>(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<reqwest::Response, ApiError>
where
    TBody: Serialize + ?Sized,
{
    let endpoint = format!(
        "{}{}",
        state
            .config
            .user_service_base_url
            .trim()
            .trim_end_matches('/'),
        path
    );
    let client = reqwest::Client::builder()
        .timeout(state.config.user_service_request_timeout)
        .build()
        .map_err(|err| ApiError::bad_gateway(format!("build user_service client failed: {err}")))?;
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("user_service request failed: {err}")))?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let message = response.text().await.unwrap_or_default();
    Err(ApiError {
        status,
        message: if message.trim().is_empty() {
            "user_service request failed".to_string()
        } else {
            message
        },
    })
}

fn user_service_user_to_task_runner_row(user: UserServiceUserSummary) -> UserSummaryRecord {
    let role = if user.role.trim() == "super_admin" {
        UserRole::Admin
    } else {
        UserRole::Agent
    };
    UserSummaryRecord {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name,
        role,
        enabled: user.enabled,
        created_at: user.created_at,
        updated_at: user.updated_at,
        last_login_at: user.last_login_at,
        principal_type: Some("human_user".to_string()),
        owner_user_id: Some(user.id),
        owner_username: Some(user.username),
        owner_display_name: None,
        agent_count: user.agent_count,
    }
}

fn user_service_agent_to_task_runner_row(agent: UserServiceAgentAccount) -> UserSummaryRecord {
    UserSummaryRecord {
        id: agent.id,
        username: agent.username,
        display_name: agent.display_name,
        role: UserRole::Agent,
        enabled: agent.enabled,
        created_at: agent.created_at,
        updated_at: agent.updated_at,
        last_login_at: agent.last_login_at,
        principal_type: Some("agent_account".to_string()),
        owner_user_id: Some(agent.owner_user_id),
        owner_username: Some(agent.owner_username),
        owner_display_name: Some(agent.owner_display_name),
        agent_count: None,
    }
}

fn task_runner_role_to_user_service_role(role: UserRole) -> String {
    match role {
        UserRole::Admin => "super_admin".to_string(),
        UserRole::Agent => "user".to_string(),
    }
}
