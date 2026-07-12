// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap, Method, Request};
use axum::middleware::Next;
use axum::response::Response;
use serde::Deserialize;

use crate::config::AppConfig;
use crate::error::ApiError;
use crate::models::{CreateSandboxLeaseRequest, ListSandboxQuery, SandboxLeaseRecord};
use crate::state::AppState;

const BEARER_PREFIX: &str = "Bearer ";
const INTERNAL_TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SERVICE_SCOPE: &str = "sandbox.service";
pub const SCOPE_ADMIN: &str = "sandbox.admin";
pub const SCOPE_POOL_READ: &str = "sandbox.pool.read";
pub const SCOPE_IMAGES_READ: &str = "sandbox.images.read";
pub const SCOPE_IMAGES_WRITE: &str = "sandbox.images.write";
pub const SCOPE_LEASE_CREATE: &str = "sandbox.lease.create";
pub const SCOPE_LEASE_READ: &str = "sandbox.lease.read";
pub const SCOPE_LEASE_RELEASE: &str = "sandbox.lease.release";
pub const SCOPE_LEASE_DESTROY: &str = "sandbox.lease.destroy";
pub const SCOPE_MCP_TOOLS: &str = "sandbox.mcp.tools";
pub const SCOPE_MCP_CALL: &str = "sandbox.mcp.call";

#[derive(Debug, Clone)]
pub struct SandboxSystemClient {
    pub client_id: String,
    pub scopes: Vec<String>,
    pub allowed_tenant_ids: Vec<String>,
    pub allowed_project_ids: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub max_lease_ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct SandboxPrincipal {
    pub principal_type: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub role: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SandboxAuthContext {
    Disabled,
    Operator,
    System(SandboxSystemClient),
    User(SandboxPrincipal),
}

#[derive(Debug, Deserialize)]
struct UserServiceVerifyResponse {
    principal: UserServicePrincipal,
}

#[derive(Debug, Deserialize)]
struct UserServicePrincipal {
    principal_type: String,
    user_id: Option<String>,
    username: Option<String>,
    role: Option<String>,
    owner_user_id: Option<String>,
    owner_username: Option<String>,
}

impl From<UserServicePrincipal> for SandboxPrincipal {
    fn from(value: UserServicePrincipal) -> Self {
        Self {
            principal_type: value.principal_type,
            user_id: value.user_id,
            username: value.username,
            role: value.role,
            owner_user_id: value.owner_user_id,
            owner_username: value.owner_username,
        }
    }
}

impl SandboxPrincipal {
    fn effective_owner_user_id(&self) -> Option<&str> {
        if self.principal_type == "agent_account" {
            return normalize_optional(self.owner_user_id.as_deref())
                .or_else(|| normalize_optional(self.user_id.as_deref()));
        }
        normalize_optional(self.user_id.as_deref())
            .or_else(|| normalize_optional(self.owner_user_id.as_deref()))
    }

    fn is_super_admin(&self) -> bool {
        self.principal_type == "human_user" && self.role.as_deref() == Some("super_admin")
    }
}

impl SandboxAuthContext {
    pub fn require_admin(&self) -> Result<(), ApiError> {
        match self {
            Self::Disabled | Self::Operator => Ok(()),
            Self::System(client) if client.has_scope(SCOPE_ADMIN) => Ok(()),
            Self::User(principal) if principal.is_super_admin() => Ok(()),
            _ => Err(ApiError::forbidden("sandbox admin permission required")),
        }
    }

    pub fn require_scope(&self, scope: &str) -> Result<(), ApiError> {
        match self {
            Self::Disabled | Self::Operator => Ok(()),
            Self::System(client) if client.has_scope(scope) => Ok(()),
            Self::System(client) if client.has_scope(SCOPE_ADMIN) => Ok(()),
            Self::User(principal) if principal.is_super_admin() => Ok(()),
            Self::User(_) if scope == SCOPE_LEASE_READ || scope == SCOPE_MCP_TOOLS => Ok(()),
            _ => Err(ApiError::forbidden(format!(
                "missing sandbox scope: {scope}"
            ))),
        }
    }

    pub fn ensure_create_lease_allowed(
        &self,
        input: &CreateSandboxLeaseRequest,
    ) -> Result<(), ApiError> {
        self.require_scope(SCOPE_LEASE_CREATE)?;
        match self {
            Self::Disabled | Self::Operator => Ok(()),
            Self::System(client) => client.ensure_create_lease_allowed(input),
            Self::User(principal) => {
                ensure_user_owns_tenant(principal, input.tenant_id.as_str())?;
                let requested_user = input.user_id.trim();
                if !requested_user.is_empty()
                    && principal
                        .effective_owner_user_id()
                        .is_some_and(|owner| owner != requested_user)
                    && !principal.is_super_admin()
                {
                    return Err(ApiError::forbidden(
                        "user_id does not match authenticated user",
                    ));
                }
                Ok(())
            }
        }
    }

    pub fn scoped_list_query(
        &self,
        mut query: ListSandboxQuery,
    ) -> Result<ListSandboxQuery, ApiError> {
        match self {
            Self::Disabled | Self::Operator => Ok(query),
            Self::System(client) => {
                client.ensure_query_allowed(&query)?;
                if query
                    .tenant_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .is_none()
                    && client.allowed_tenant_ids.len() == 1
                    && !is_wildcard_list(&client.allowed_tenant_ids)
                {
                    query.tenant_id = client.allowed_tenant_ids.first().cloned();
                }
                Ok(query)
            }
            Self::User(principal) => {
                if principal.is_super_admin() {
                    return Ok(query);
                }
                let owner = principal.effective_owner_user_id().ok_or_else(|| {
                    ApiError::unauthorized("authenticated principal has no owner scope")
                })?;
                if let Some(requested) = normalize_optional(query.tenant_id.as_deref()) {
                    if requested != owner {
                        return Err(ApiError::forbidden(
                            "tenant_id does not match authenticated user",
                        ));
                    }
                }
                query.tenant_id = Some(owner.to_string());
                Ok(query)
            }
        }
    }

    pub fn ensure_lease_access(
        &self,
        record: &SandboxLeaseRecord,
        scope: &str,
    ) -> Result<(), ApiError> {
        self.require_scope(scope)?;
        match self {
            Self::Disabled | Self::Operator => Ok(()),
            Self::System(client) => client.ensure_lease_allowed(record),
            Self::User(principal) => {
                if principal.is_super_admin() {
                    return Ok(());
                }
                ensure_user_owns_tenant(principal, record.tenant_id.as_str())
            }
        }
    }

    pub fn ensure_tool_allowed(&self, tool_name: &str) -> Result<(), ApiError> {
        match self {
            Self::System(client) => client.ensure_tool_allowed(tool_name),
            _ => Ok(()),
        }
    }
}

impl SandboxSystemClient {
    fn has_scope(&self, scope: &str) -> bool {
        self.scopes
            .iter()
            .any(|value| value == "*" || value == scope || value == SCOPE_ADMIN)
    }

    fn ensure_create_lease_allowed(
        &self,
        input: &CreateSandboxLeaseRequest,
    ) -> Result<(), ApiError> {
        ensure_value_allowed(
            "tenant_id",
            input.tenant_id.as_str(),
            &self.allowed_tenant_ids,
        )?;
        ensure_value_allowed(
            "project_id",
            input.project_id.as_str(),
            &self.allowed_project_ids,
        )?;
        if let Some(ttl_seconds) = input.ttl_seconds {
            if ttl_seconds > self.max_lease_ttl_seconds {
                return Err(ApiError::forbidden(format!(
                    "ttl_seconds exceeds client policy: requested={ttl_seconds}, max={}",
                    self.max_lease_ttl_seconds
                )));
            }
        }
        for tool in &input.tools {
            self.ensure_tool_allowed(tool)?;
        }
        Ok(())
    }

    fn ensure_query_allowed(&self, query: &ListSandboxQuery) -> Result<(), ApiError> {
        if let Some(tenant_id) = normalize_optional(query.tenant_id.as_deref()) {
            ensure_value_allowed("tenant_id", tenant_id, &self.allowed_tenant_ids)?;
        } else if !is_wildcard_list(&self.allowed_tenant_ids) && self.allowed_tenant_ids.len() != 1
        {
            return Err(ApiError::bad_request(
                "tenant_id is required for this sandbox client",
            ));
        }
        if let Some(project_id) = normalize_optional(query.project_id.as_deref()) {
            ensure_value_allowed("project_id", project_id, &self.allowed_project_ids)?;
        }
        Ok(())
    }

    fn ensure_lease_allowed(&self, record: &SandboxLeaseRecord) -> Result<(), ApiError> {
        ensure_value_allowed(
            "tenant_id",
            record.tenant_id.as_str(),
            &self.allowed_tenant_ids,
        )?;
        ensure_value_allowed(
            "project_id",
            record.project_id.as_str(),
            &self.allowed_project_ids,
        )?;
        Ok(())
    }

    fn ensure_tool_allowed(&self, tool_name: &str) -> Result<(), ApiError> {
        ensure_value_allowed("tool", tool_name, &self.allowed_tools)
    }
}

pub async fn require_sandbox_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    let auth = authenticate_request(&state, request.headers()).await?;
    request.extensions_mut().insert(auth);
    Ok(next.run(request).await)
}

async fn authenticate_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SandboxAuthContext, ApiError> {
    let config = state.manager.config();
    if !config.require_auth {
        return Ok(SandboxAuthContext::Disabled);
    }

    if let Some(client) = authenticate_internal_service(config, headers)? {
        return Ok(SandboxAuthContext::System(client));
    }
    if let Some(client) = authenticate_system_client(state, config, headers).await? {
        return Ok(SandboxAuthContext::System(client));
    }
    if authenticate_operator(config, headers)? {
        return Ok(SandboxAuthContext::Operator);
    }
    if let Some(token) = bearer_token(headers)? {
        return verify_user_service_principal(config, token).await;
    }

    Err(ApiError::unauthorized("missing sandbox authorization"))
}

fn authenticate_internal_service(
    config: &AppConfig,
    headers: &HeaderMap,
) -> Result<Option<SandboxSystemClient>, ApiError> {
    let caller = header_text(headers, "x-sandbox-caller");
    let token = header_text(headers, "x-sandbox-internal-token");
    if caller.is_none() && token.is_none() {
        return Ok(None);
    }
    let caller = caller.ok_or_else(|| {
        ApiError::bad_request("Sandbox Manager caller is required for signed internal requests")
    })?;
    let token = token.ok_or_else(|| {
        ApiError::unauthorized("signed Sandbox Manager internal API token is required")
    })?;
    let secret = config
        .internal_api_secrets
        .get(caller.as_str())
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unauthorized("Sandbox Manager internal API is disabled for caller")
        })?;
    chatos_service_runtime::verify_internal_service_token(
        token.as_str(),
        secret,
        caller.as_str(),
        INTERNAL_TOKEN_AUDIENCE,
        INTERNAL_SERVICE_SCOPE,
    )
    .map_err(|_| ApiError::unauthorized("invalid Sandbox Manager internal API token"))?;

    let scopes = match caller.as_str() {
        "task-runner" => vec![
            SCOPE_LEASE_CREATE,
            SCOPE_LEASE_READ,
            SCOPE_LEASE_RELEASE,
            SCOPE_MCP_TOOLS,
            SCOPE_MCP_CALL,
            SCOPE_POOL_READ,
            SCOPE_IMAGES_READ,
        ],
        "project-service" => vec![SCOPE_IMAGES_READ, SCOPE_IMAGES_WRITE],
        _ => {
            return Err(ApiError::forbidden(
                "caller service is not allowed for Sandbox Manager",
            ));
        }
    };
    Ok(Some(SandboxSystemClient {
        client_id: caller,
        scopes: scopes.into_iter().map(ToOwned::to_owned).collect(),
        allowed_tenant_ids: vec!["*".to_string()],
        allowed_project_ids: vec!["*".to_string()],
        allowed_tools: vec!["*".to_string()],
        max_lease_ttl_seconds: config.system_client_max_lease_ttl_seconds,
    }))
}

async fn authenticate_system_client(
    state: &AppState,
    config: &AppConfig,
    headers: &HeaderMap,
) -> Result<Option<SandboxSystemClient>, ApiError> {
    let Some(client_id) = header_text(headers, "x-sandbox-client-id") else {
        return Ok(None);
    };
    let Some(client_key) = header_text(headers, "x-sandbox-client-key") else {
        return Err(ApiError::unauthorized("missing x-sandbox-client-key"));
    };
    if let Some(client) = state
        .manager
        .authenticate_access_client(client_id.as_str(), client_key.as_str())
        .await?
    {
        return Ok(Some(client));
    }

    if config.require_signed_internal_requests {
        return Err(ApiError::unauthorized(
            "signed Sandbox Manager internal API token is required",
        ));
    }

    let expected_id = config
        .system_client_id
        .as_deref()
        .ok_or_else(|| ApiError::unauthorized("sandbox system client is not configured"))?;
    let expected_key = config
        .system_client_key
        .as_deref()
        .ok_or_else(|| ApiError::unauthorized("sandbox system client key is not configured"))?;
    if !constant_time_equal(expected_id, client_id.as_str())
        || !constant_time_equal(expected_key, client_key.as_str())
    {
        return Err(ApiError::unauthorized("invalid sandbox system credentials"));
    }

    Ok(Some(SandboxSystemClient {
        client_id,
        scopes: config.system_client_scopes.clone(),
        allowed_tenant_ids: config.system_client_allowed_tenant_ids.clone(),
        allowed_project_ids: config.system_client_allowed_project_ids.clone(),
        allowed_tools: config.system_client_allowed_tools.clone(),
        max_lease_ttl_seconds: config.system_client_max_lease_ttl_seconds,
    }))
}

fn authenticate_operator(config: &AppConfig, headers: &HeaderMap) -> Result<bool, ApiError> {
    let Some(expected) = config.operator_token.as_deref() else {
        return Ok(false);
    };
    let Some(provided) = header_text(headers, "x-sandbox-operator-token") else {
        return Ok(false);
    };
    if constant_time_equal(expected, provided.as_str()) {
        Ok(true)
    } else {
        Err(ApiError::unauthorized("invalid sandbox operator token"))
    }
}

fn bearer_token(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| ApiError::unauthorized("invalid authorization header"))?;
    let token = value
        .strip_prefix(BEARER_PREFIX)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("invalid authorization header"))?;
    Ok(Some(token))
}

async fn verify_user_service_principal(
    config: &AppConfig,
    token: &str,
) -> Result<SandboxAuthContext, ApiError> {
    let endpoint = format!(
        "{}/api/auth/verify",
        config.user_service_base_url.trim().trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(
            config.user_service_request_timeout_ms.max(300),
        ))
        .build()
        .map_err(|err| {
            ApiError::with_code(
                axum::http::StatusCode::BAD_GATEWAY,
                "user_service_client_error",
                format!("build user_service client failed: {err}"),
            )
        })?;
    let response = client
        .get(endpoint)
        .bearer_auth(token.trim())
        .send()
        .await
        .map_err(|err| {
            ApiError::with_code(
                axum::http::StatusCode::BAD_GATEWAY,
                "user_service_verify_failed",
                format!("verify token via user_service failed: {err}"),
            )
        })?;
    if !response.status().is_success() {
        return Err(ApiError::unauthorized("invalid user token"));
    }
    let payload = response
        .json::<UserServiceVerifyResponse>()
        .await
        .map_err(|err| {
            ApiError::with_code(
                axum::http::StatusCode::BAD_GATEWAY,
                "user_service_verify_invalid_response",
                format!("parse user_service verify response failed: {err}"),
            )
        })?;
    Ok(SandboxAuthContext::User(payload.principal.into()))
}

fn ensure_user_owns_tenant(principal: &SandboxPrincipal, tenant_id: &str) -> Result<(), ApiError> {
    if principal.is_super_admin() {
        return Ok(());
    }
    let owner = principal
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("authenticated principal has no owner scope"))?;
    if tenant_id.trim() == owner {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "tenant_id does not match authenticated user",
        ))
    }
}

fn ensure_value_allowed(name: &str, value: &str, allowed: &[String]) -> Result<(), ApiError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(ApiError::bad_request(format!("{name} is required")));
    }
    if is_wildcard_list(allowed)
        || allowed
            .iter()
            .map(|item| item.trim())
            .any(|item| item == normalized)
    {
        return Ok(());
    }
    Err(ApiError::forbidden(format!(
        "{name} is not allowed for this sandbox client"
    )))
}

fn is_wildcard_list(values: &[String]) -> bool {
    values.is_empty() || values.iter().any(|value| value.trim() == "*")
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn constant_time_equal(expected: &str, provided: &str) -> bool {
    let expected = expected.as_bytes();
    let provided = provided.as_bytes();
    if expected.len() != provided.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in expected.iter().zip(provided.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}
