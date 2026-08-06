// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request};
use axum::middleware::Next;
use axum::response::Response;
use chatos_service_runtime::http_body::{read_response_json_limited, JSON_BODY_LIMIT_BYTES};
use chatos_service_runtime::{
    bearer_token_from_headers, classify_http_request_error,
    normalized_identity_text as normalize_optional, BearerTokenError, HttpRequestErrorKind,
};
use serde::Deserialize;

use crate::config::AppConfig;
use crate::error::ApiError;
use crate::models::{
    CreateSandboxEnvironmentLeaseRequest, CreateSandboxLeaseRequest, ListSandboxQuery,
    SandboxLeaseRecord,
};
use crate::state::AppState;

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
    pub internal_identity: Option<SandboxInternalIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxInternalIdentity {
    pub caller_service: String,
    pub scope: String,
    pub trace_id: String,
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
    pub fn internal_identity(&self) -> Option<&SandboxInternalIdentity> {
        match self {
            Self::System(client) => client.internal_identity.as_ref(),
            Self::User(_) => None,
        }
    }

    pub fn system_client_id(&self) -> Option<&str> {
        match self {
            Self::System(client) => Some(client.client_id.as_str()),
            _ => None,
        }
    }

    pub fn require_admin(&self) -> Result<(), ApiError> {
        match self {
            Self::System(client) if client.has_scope(SCOPE_ADMIN) => Ok(()),
            Self::User(principal) if principal.is_super_admin() => Ok(()),
            _ => Err(ApiError::forbidden("sandbox admin permission required")),
        }
    }

    pub fn require_scope(&self, scope: &str) -> Result<(), ApiError> {
        match self {
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

    pub fn ensure_create_environment_lease_allowed(
        &self,
        input: &CreateSandboxEnvironmentLeaseRequest,
    ) -> Result<(), ApiError> {
        self.require_scope(SCOPE_LEASE_CREATE)?;
        match self {
            Self::System(client) => client.ensure_create_environment_lease_allowed(input),
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
            Self::System(client) => client.ensure_lease_allowed(record),
            Self::User(principal) => {
                if principal.is_super_admin() {
                    return Ok(());
                }
                ensure_user_owns_tenant(principal, record.tenant_id.as_str())
            }
        }
    }

    pub fn ensure_lease_renewal_allowed(
        &self,
        record: &SandboxLeaseRecord,
        ttl_seconds: u64,
    ) -> Result<(), ApiError> {
        self.ensure_lease_access(record, SCOPE_LEASE_CREATE)?;
        if let Self::System(client) = self {
            if ttl_seconds > client.max_lease_ttl_seconds {
                return Err(ApiError::forbidden(format!(
                    "ttl_seconds exceeds client policy: requested={ttl_seconds}, max={}",
                    client.max_lease_ttl_seconds
                )));
            }
        }
        Ok(())
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

    fn ensure_create_environment_lease_allowed(
        &self,
        input: &CreateSandboxEnvironmentLeaseRequest,
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

pub async fn require_public_sandbox_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    let auth = authenticate_public_request(&state, request.headers()).await?;
    request.extensions_mut().insert(auth);
    Ok(next.run(request).await)
}

pub async fn require_internal_sandbox_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let method = request.method().clone();
    let resource_path = request.uri().path().to_string();
    let auth = authenticate_internal_request(&state, request.headers())?;
    let audit_event = auth.internal_identity().map(|identity| {
        sandbox_internal_audit_event(
            identity,
            request.headers(),
            method.as_str(),
            resource_path.as_str(),
            "pending",
        )
    });
    request.extensions_mut().insert(auth);
    let response = next.run(request).await;
    if let Some(mut event) = audit_event {
        event.outcome = response.status().as_u16().to_string();
        if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
            tracing::error!(
                target: "chatos_internal_audit",
                trace_id = event.trace_id.as_str(),
                error = error.as_str(),
                "record Sandbox Manager internal access audit failed"
            );
        }
    }
    Ok(response)
}

fn sandbox_internal_audit_event(
    identity: &SandboxInternalIdentity,
    headers: &HeaderMap,
    method: &str,
    resource_path: &str,
    outcome: &str,
) -> chatos_service_runtime::InternalResourceAccessAudit {
    let represented_user_id = match identity.caller_service.as_str() {
        "task-runner" => first_audit_header(
            headers,
            &["x-chatos-owner-user-id", "x-task-runner-owner-user-id"],
        ),
        "mcp-management-service" => {
            first_audit_header(headers, &["x-mcp-management-owner-user-id"])
        }
        "project-service" => first_audit_header(headers, &["x-project-service-owner-user-id"]),
        _ => None,
    };
    let tenant_id = match identity.caller_service.as_str() {
        "task-runner" => first_audit_header(headers, &["x-chatos-tenant-id"]),
        _ => None,
    };
    let project_id = match identity.caller_service.as_str() {
        "task-runner" => first_audit_header(headers, &["x-chatos-project-id"]),
        "mcp-management-service" => first_audit_header(headers, &["x-mcp-management-project-id"]),
        "project-service" => first_audit_header(
            headers,
            &[
                "x-chatos-sandbox-project-id",
                "x-project-service-project-id",
            ],
        ),
        _ => None,
    };

    chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: identity.caller_service.clone(),
        audience_service: INTERNAL_TOKEN_AUDIENCE.to_string(),
        scope: identity.scope.clone(),
        trace_id: identity.trace_id.clone(),
        represented_user_id,
        tenant_id,
        project_id,
        resource_type: sandbox_resource_type(resource_path).to_string(),
        resource_id: bounded_audit_text(resource_path),
        resource_name: None,
        action: bounded_audit_text(method),
        outcome: bounded_audit_text(outcome),
    }
}

fn sandbox_resource_type(path: &str) -> &'static str {
    let path = path
        .strip_prefix("/api/internal")
        .or_else(|| path.strip_prefix("/api"))
        .unwrap_or(path);
    if path.starts_with("/sandbox-environments/") {
        "sandbox_environment"
    } else if path.starts_with("/sandboxes/") {
        "sandbox"
    } else if path.starts_with("/sandbox-images") {
        "sandbox_image_control_plane"
    } else if path.starts_with("/sandbox-pool") {
        "sandbox_pool"
    } else if path.starts_with("/access-clients") {
        "sandbox_access_client"
    } else {
        "sandbox_internal_route"
    }
}

fn first_audit_header(headers: &HeaderMap, names: &[&'static str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| header_text(headers, name))
        .map(|value| bounded_audit_text(value.as_str()))
}

fn bounded_audit_text(value: &str) -> String {
    value.trim().chars().take(256).collect()
}

async fn authenticate_public_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SandboxAuthContext, ApiError> {
    if header_text(headers, "x-sandbox-caller").is_some()
        || header_text(headers, "x-sandbox-internal-token").is_some()
    {
        return Err(ApiError::unauthorized(
            "signed internal requests are not accepted on the public listener",
        ));
    }
    if let Some(client) = authenticate_system_client(state, headers).await? {
        return Ok(SandboxAuthContext::System(client));
    }
    if let Some(token) = bearer_token(headers)? {
        return verify_user_service_principal(
            state.manager.config(),
            state.user_service_http(),
            token,
        )
        .await;
    }

    Err(ApiError::unauthorized("missing sandbox authorization"))
}

fn authenticate_internal_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SandboxAuthContext, ApiError> {
    authenticate_internal_service(state.manager.config(), headers)?
        .map(SandboxAuthContext::System)
        .ok_or_else(|| ApiError::unauthorized("signed Sandbox Manager internal request required"))
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
    let claims = chatos_service_runtime::verify_internal_service_token(
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
        "mcp-management-service" => vec![
            SCOPE_LEASE_READ,
            SCOPE_MCP_TOOLS,
            SCOPE_MCP_CALL,
            SCOPE_IMAGES_READ,
            SCOPE_IMAGES_WRITE,
        ],
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
        internal_identity: Some(SandboxInternalIdentity {
            caller_service: claims.caller,
            scope: claims.scope,
            trace_id: claims.trace_id,
        }),
    }))
}

async fn authenticate_system_client(
    state: &AppState,
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

    Err(ApiError::unauthorized("invalid sandbox system credentials"))
}

fn bearer_token(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    match bearer_token_from_headers(headers) {
        Ok(token) => Ok(Some(token)),
        Err(BearerTokenError::MissingAuthorizationHeader) => Ok(None),
        Err(
            BearerTokenError::InvalidAuthorizationHeader | BearerTokenError::InvalidBearerToken,
        ) => Err(ApiError::unauthorized("invalid authorization header")),
    }
}

async fn verify_user_service_principal(
    config: &AppConfig,
    client: &reqwest::Client,
    token: &str,
) -> Result<SandboxAuthContext, ApiError> {
    let endpoint = format!(
        "{}/api/auth/verify",
        config.user_service_base_url.trim().trim_end_matches('/')
    );
    let response = client
        .get(endpoint)
        .bearer_auth(token.trim())
        .send()
        .await
        .map_err(|err| {
            let status = if classify_http_request_error(&err) == HttpRequestErrorKind::Timeout {
                axum::http::StatusCode::GATEWAY_TIMEOUT
            } else {
                axum::http::StatusCode::BAD_GATEWAY
            };
            ApiError::with_code(
                status,
                "user_service_verify_failed",
                format!("verify token via user_service failed: {err}"),
            )
        })?;
    if !response.status().is_success() {
        return Err(ApiError::unauthorized("invalid user token"));
    }
    let payload =
        read_response_json_limited::<UserServiceVerifyResponse>(response, JSON_BODY_LIMIT_BYTES)
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

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::*;

    #[test]
    fn signed_internal_request_preserves_verified_audit_identity() {
        let config = AppConfig::for_tests();
        let secret = config
            .internal_api_secrets
            .get("task-runner")
            .expect("task runner secret");
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            "task-runner",
            INTERNAL_TOKEN_AUDIENCE,
            INTERNAL_SERVICE_SCOPE,
            60,
        )
        .expect("signed token");
        let mut headers = HeaderMap::new();
        headers.insert("x-sandbox-caller", HeaderValue::from_static("task-runner"));
        headers.insert(
            "x-sandbox-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );

        let client = authenticate_internal_service(&config, &headers)
            .expect("authentication result")
            .expect("internal client");
        let identity = client.internal_identity.expect("audit identity");

        assert_eq!(identity.caller_service, "task-runner");
        assert_eq!(identity.scope, INTERNAL_SERVICE_SCOPE);
        assert!(uuid::Uuid::parse_str(identity.trace_id.as_str()).is_ok());
    }

    #[test]
    fn audit_context_only_accepts_headers_for_the_verified_caller() {
        let identity = SandboxInternalIdentity {
            caller_service: "task-runner".to_string(),
            scope: INTERNAL_SERVICE_SCOPE.to_string(),
            trace_id: uuid::Uuid::new_v4().to_string(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-chatos-owner-user-id",
            HeaderValue::from_static("task-user"),
        );
        headers.insert("x-chatos-tenant-id", HeaderValue::from_static("tenant-1"));
        headers.insert("x-chatos-project-id", HeaderValue::from_static("project-1"));
        headers.insert(
            "x-mcp-management-owner-user-id",
            HeaderValue::from_static("spoofed-user"),
        );
        headers.insert(
            "x-mcp-management-project-id",
            HeaderValue::from_static("spoofed-project"),
        );

        let event = sandbox_internal_audit_event(
            &identity,
            &headers,
            "POST",
            "/api/sandboxes/leases",
            "201",
        );

        assert_eq!(event.represented_user_id.as_deref(), Some("task-user"));
        assert_eq!(event.tenant_id.as_deref(), Some("tenant-1"));
        assert_eq!(event.project_id.as_deref(), Some("project-1"));
        assert_eq!(event.resource_type, "sandbox");
        assert!(event.validate().is_ok());
    }
}
