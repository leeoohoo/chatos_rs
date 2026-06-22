use super::*;

pub(in crate::api) async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let current_user = current_user_from_request(&request, &state)?;
    let path = request.uri().path();
    if !current_user.is_admin() && path != "/api/auth/me" && path != "/api/auth/logout" {
        return Err(ApiError::forbidden("当前账号不能访问管理后台接口"));
    }
    request.extensions_mut().insert(current_user);
    Ok(next.run(request).await)
}

pub(in crate::api) async fn login_handler(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let response = state
        .auth_service
        .login(input.username.as_str(), input.password.as_str())
        .await
        .map_err(ApiError::unauthorized)?;
    Ok(Json(response))
}

pub(in crate::api) async fn agent_token_handler(
    State(state): State<AppState>,
    Json(input): Json<AgentTokenRequest>,
) -> Result<Json<AgentTokenResponse>, ApiError> {
    let response = state
        .auth_service
        .issue_agent_token(input.username.as_str(), input.password.as_str())
        .await
        .map_err(ApiError::unauthorized)?;
    Ok(Json(response))
}

pub(in crate::api) async fn current_user_handler(
    Extension(current_user): Extension<CurrentUser>,
) -> Json<CurrentUserResponse> {
    Json(CurrentUserResponse {
        user: current_user.public_user(),
    })
}

pub(in crate::api) async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = bearer_token_from_headers(&headers).map_err(ApiError::unauthorized)?;
    state.auth_service.logout(token);
    Ok(StatusCode::NO_CONTENT)
}

fn current_user_from_request(request: &Request, state: &AppState) -> Result<CurrentUser, ApiError> {
    let token = bearer_token_from_request(request).map_err(ApiError::unauthorized)?;
    state
        .auth_service
        .current_user_for_token(token)
        .ok_or_else(|| ApiError::unauthorized("登录已失效，请重新登录"))
}

fn bearer_token_from_request(request: &Request) -> Result<&str, String> {
    bearer_token_from_headers(request.headers()).or_else(|_| {
        token_from_query(request.uri().query()).ok_or_else(|| "缺少登录令牌".to_string())
    })
}

pub(in crate::api) fn bearer_token_from_headers(headers: &HeaderMap) -> Result<&str, String> {
    let value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| "缺少登录令牌".to_string())?
        .to_str()
        .map_err(|_| "登录令牌格式不正确".to_string())?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next().unwrap_or_default();
    let token = parts.next().unwrap_or_default();
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || parts.next().is_some() {
        return Err("登录令牌格式不正确".to_string());
    }
    Ok(token)
}

fn token_from_query(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?.trim();
        ((key == "access_token" || key == "token") && !value.is_empty()).then_some(value)
    })
}
