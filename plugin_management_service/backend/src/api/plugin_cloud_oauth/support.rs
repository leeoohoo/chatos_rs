// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn discover_oauth_server(
    state: &AppState,
    resource: &str,
    requested_authorization_server: Option<&str>,
    scopes: &[String],
) -> Result<DiscoveredOAuthServer, ApiError> {
    let resource_url = validate_public_https_url(resource, "Plugin OAuth resource")?;
    let mut resource_metadata = None;
    for url in protected_resource_metadata_urls(&resource_url)? {
        if let Some(metadata) =
            fetch_json_optional::<ProtectedResourceMetadata>(state, &url).await?
        {
            resource_metadata = Some(metadata);
            break;
        }
    }
    let resource_metadata = resource_metadata.ok_or_else(|| {
        ApiError::bad_gateway("Plugin OAuth protected resource metadata was not found")
    })?;
    if let Some(metadata_resource) = resource_metadata.resource.as_deref() {
        let metadata_resource = validate_public_https_url(
            metadata_resource,
            "Plugin OAuth protected resource metadata resource",
        )?;
        if normalized_url(&metadata_resource) != normalized_url(&resource_url) {
            return Err(ApiError::conflict(
                "Plugin OAuth protected resource metadata does not match the immutable resource",
            ));
        }
    }
    if !resource_metadata.scopes_supported.is_empty() {
        let supported = normalize_scopes(resource_metadata.scopes_supported)?;
        if scopes.iter().any(|scope| !supported.contains(scope)) {
            return Err(ApiError::conflict(
                "Plugin OAuth requested scopes are not supported by the protected resource",
            ));
        }
    }
    let authorization_servers = resource_metadata
        .authorization_servers
        .into_iter()
        .map(|value| validate_public_https_url(value.as_str(), "OAuth authorization server"))
        .collect::<Result<Vec<_>, _>>()?;
    if authorization_servers.is_empty() {
        return Err(ApiError::bad_gateway(
            "Plugin OAuth protected resource did not publish an authorization server",
        ));
    }
    let authorization_server = match requested_authorization_server {
        Some(value) => {
            let requested = validate_public_https_url(value, "authorization_server")?;
            authorization_servers
                .into_iter()
                .find(|candidate| normalized_url(candidate) == normalized_url(&requested))
                .ok_or_else(|| {
                    ApiError::conflict(
                        "Requested OAuth authorization server is not authorized by the protected resource",
                    )
                })?
        }
        None if authorization_servers.len() == 1 => authorization_servers
            .into_iter()
            .next()
            .expect("single authorization server"),
        None => {
            return Err(ApiError::bad_request(
                "Plugin OAuth resource publishes multiple authorization servers; select one explicitly",
            ));
        }
    };
    let mut server_metadata = None;
    for url in authorization_server_metadata_urls(&authorization_server)? {
        if let Some(metadata) =
            fetch_json_optional::<AuthorizationServerMetadata>(state, &url).await?
        {
            server_metadata = Some(metadata);
            break;
        }
    }
    let metadata = server_metadata.ok_or_else(|| {
        ApiError::bad_gateway("OAuth authorization server metadata was not found")
    })?;
    let issuer = validate_public_https_url(metadata.issuer.as_str(), "OAuth issuer")?;
    if normalized_url(&issuer) != normalized_url(&authorization_server) {
        return Err(ApiError::conflict(
            "OAuth authorization server metadata issuer does not match the protected resource",
        ));
    }
    if !metadata.response_types_supported.is_empty()
        && !metadata
            .response_types_supported
            .iter()
            .any(|value| value == "code")
    {
        return Err(ApiError::conflict(
            "OAuth authorization server does not support authorization code flow",
        ));
    }
    if !metadata.grant_types_supported.is_empty()
        && !metadata
            .grant_types_supported
            .iter()
            .any(|value| value == "authorization_code")
    {
        return Err(ApiError::conflict(
            "OAuth authorization server does not support authorization code grant",
        ));
    }
    if !metadata.code_challenge_methods_supported.is_empty()
        && !metadata
            .code_challenge_methods_supported
            .iter()
            .any(|value| value == "S256")
    {
        return Err(ApiError::conflict(
            "OAuth authorization server does not support PKCE S256",
        ));
    }
    let authorization_endpoint = validate_public_https_url(
        metadata.authorization_endpoint.as_str(),
        "OAuth authorization endpoint",
    )?;
    ensure_public_url_host(&authorization_endpoint).await?;
    let token_endpoint =
        validate_public_https_url(metadata.token_endpoint.as_str(), "OAuth token endpoint")?;
    ensure_public_url_host(&token_endpoint).await?;
    let registration_endpoint = metadata
        .registration_endpoint
        .as_deref()
        .map(|value| validate_public_https_url(value, "OAuth registration endpoint"))
        .transpose()?;
    if let Some(endpoint) = registration_endpoint.as_ref() {
        ensure_public_url_host(endpoint).await?;
    }
    Ok(DiscoveredOAuthServer {
        authorization_server: authorization_server.to_string(),
        authorization_endpoint: authorization_endpoint.to_string(),
        token_endpoint: token_endpoint.to_string(),
        registration_endpoint: registration_endpoint.map(|url| url.to_string()),
        token_endpoint_auth_methods_supported: metadata.token_endpoint_auth_methods_supported,
    })
}

pub(super) async fn resolve_oauth_client(
    state: &AppState,
    server: &DiscoveredOAuthServer,
    redirect_uri: &str,
    client_id: Option<String>,
    client_secret: Option<String>,
    requested_auth_method: Option<String>,
) -> Result<OAuthClientRegistration, ApiError> {
    if let Some(client_id) = client_id {
        let client_id = validate_oauth_text(client_id.as_str(), "client_id", 1_024)?;
        let client_secret = client_secret
            .map(|value| validate_token_secret(value, "OAuth client secret"))
            .transpose()?;
        let method = normalize_token_endpoint_auth_method(
            requested_auth_method.as_deref(),
            client_secret.is_some(),
        )?;
        require_supported_auth_method(
            method.as_str(),
            server.token_endpoint_auth_methods_supported.as_slice(),
        )?;
        return Ok(OAuthClientRegistration {
            client_id,
            client_secret,
            token_endpoint_auth_method: method,
        });
    }
    if client_secret.is_some() || requested_auth_method.is_some() {
        return Err(ApiError::bad_request(
            "OAuth client_secret and token_endpoint_auth_method require client_id",
        ));
    }
    let registration_endpoint = server.registration_endpoint.as_deref().ok_or_else(|| {
        ApiError::conflict(
            "OAuth server does not support dynamic client registration; configure client_id",
        )
    })?;
    require_supported_auth_method(
        "none",
        server.token_endpoint_auth_methods_supported.as_slice(),
    )?;
    let response: DynamicClientRegistrationResponse = post_json(
        state,
        &validate_public_https_url(registration_endpoint, "OAuth registration endpoint")?,
        &DynamicClientRegistrationRequest {
            client_name: "ChatOS Plugin MCP",
            redirect_uris: [redirect_uri],
            grant_types: ["authorization_code", "refresh_token"],
            response_types: ["code"],
            token_endpoint_auth_method: "none",
        },
    )
    .await?;
    let client_id = validate_oauth_text(response.client_id.as_str(), "client_id", 1_024)?;
    let client_secret = response
        .client_secret
        .map(|value| validate_token_secret(value, "OAuth client secret"))
        .transpose()?;
    let method = normalize_token_endpoint_auth_method(
        response.token_endpoint_auth_method.as_deref(),
        client_secret.is_some(),
    )?;
    require_supported_auth_method(
        method.as_str(),
        server.token_endpoint_auth_methods_supported.as_slice(),
    )?;
    Ok(OAuthClientRegistration {
        client_id,
        client_secret,
        token_endpoint_auth_method: method,
    })
}

pub(super) fn build_authorization_url(
    endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
    resource: &str,
    scopes: &[String],
) -> Result<Url, ApiError> {
    let mut url = validate_public_https_url(endpoint, "OAuth authorization endpoint")?;
    {
        let mut query = url.query_pairs_mut();
        query.clear();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("state", state);
        query.append_pair("code_challenge", code_challenge);
        query.append_pair("code_challenge_method", "S256");
        query.append_pair("resource", resource);
        query.append_pair("scope", scopes.join(" ").as_str());
    }
    Ok(url)
}

pub(super) async fn request_oauth_token(
    state: &AppState,
    endpoint: &str,
    client_id: &str,
    client_secret: Option<&str>,
    auth_method: &str,
    mut fields: Vec<(String, String)>,
) -> Result<OAuthTokenResponse, OAuthTokenRequestError> {
    let url = validate_public_https_url(endpoint, "OAuth token endpoint")
        .map_err(|error| OAuthTokenRequestError::Transient(error.message))?;
    let client = public_http_client(state, &url)
        .await
        .map_err(|error| OAuthTokenRequestError::Transient(error.message))?;
    let mut request = client.post(url);
    match auth_method {
        "none" => fields.push(("client_id".to_string(), client_id.to_string())),
        "client_secret_post" => {
            let secret = client_secret.ok_or(OAuthTokenRequestError::ReauthorizationRequired)?;
            fields.push(("client_id".to_string(), client_id.to_string()));
            fields.push(("client_secret".to_string(), secret.to_string()));
        }
        "client_secret_basic" => {
            let secret = client_secret.ok_or(OAuthTokenRequestError::ReauthorizationRequired)?;
            request = request.basic_auth(client_id, Some(secret));
        }
        _ => return Err(OAuthTokenRequestError::ReauthorizationRequired),
    }
    request = request.form(&fields);
    for (_, value) in &mut fields {
        value.zeroize();
    }
    let response = request
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| {
            OAuthTokenRequestError::Transient("OAuth token endpoint request failed".to_string())
        })?;
    let status = response.status();
    let body = Zeroizing::new(
        bounded_response_body(response, state.config.oauth_max_response_bytes)
            .await
            .map_err(|error| OAuthTokenRequestError::Transient(error.message))?,
    );
    if !status.is_success() {
        let error = serde_json::from_slice::<OAuthErrorResponse>(body.as_slice())
            .ok()
            .and_then(|payload| payload.error)
            .unwrap_or_default();
        if status == HttpStatusCode::BAD_REQUEST
            || status == HttpStatusCode::UNAUTHORIZED
            || matches!(
                error.as_str(),
                "invalid_grant" | "invalid_client" | "unauthorized_client"
            )
        {
            return Err(OAuthTokenRequestError::ReauthorizationRequired);
        }
        return Err(OAuthTokenRequestError::Transient(format!(
            "OAuth token endpoint returned {status}"
        )));
    }
    serde_json::from_slice(body.as_slice()).map_err(|_| {
        OAuthTokenRequestError::Transient("OAuth token response is invalid".to_string())
    })
}

pub(super) async fn fetch_json_optional<T: DeserializeOwned>(
    state: &AppState,
    url: &Url,
) -> Result<Option<T>, ApiError> {
    let client = public_http_client(state, url).await?;
    let response = client
        .get(url.clone())
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("OAuth metadata request failed"))?;
    if response.status() == HttpStatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(ApiError::bad_gateway(format!(
            "OAuth metadata endpoint returned {}",
            response.status()
        )));
    }
    let body = Zeroizing::new(
        bounded_response_body(response, state.config.oauth_max_response_bytes).await?,
    );
    serde_json::from_slice(body.as_slice())
        .map(Some)
        .map_err(|_| ApiError::bad_gateway("OAuth metadata response is invalid"))
}

pub(super) async fn post_json<T: DeserializeOwned, B: Serialize + ?Sized>(
    state: &AppState,
    url: &Url,
    body: &B,
) -> Result<T, ApiError> {
    let client = public_http_client(state, url).await?;
    let response = client
        .post(url.clone())
        .header(header::ACCEPT, "application/json")
        .json(body)
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("OAuth client registration request failed"))?;
    if !response.status().is_success() {
        return Err(ApiError::bad_gateway(format!(
            "OAuth client registration returned {}",
            response.status()
        )));
    }
    let body = Zeroizing::new(
        bounded_response_body(response, state.config.oauth_max_response_bytes).await?,
    );
    serde_json::from_slice(body.as_slice())
        .map_err(|_| ApiError::bad_gateway("OAuth client registration response is invalid"))
}

pub(super) async fn public_http_client(
    state: &AppState,
    url: &Url,
) -> Result<reqwest::Client, ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::bad_request("OAuth URL host is missing"))?;
    let addresses = resolve_public_url_addresses(url).await?;
    reqwest::Client::builder()
        .timeout(state.config.oauth_request_timeout)
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .resolve_to_addrs(host, addresses.as_slice())
        .build()
        .map_err(|error| ApiError::internal(format!("build OAuth HTTP client failed: {error}")))
}

pub(super) async fn ensure_public_url_host(url: &Url) -> Result<(), ApiError> {
    resolve_public_url_addresses(url).await.map(|_| ())
}

pub(super) async fn resolve_public_url_addresses(url: &Url) -> Result<Vec<SocketAddr>, ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::bad_request("OAuth URL host is missing"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ApiError::bad_request("OAuth URL port is invalid"))?;
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ApiError::bad_gateway("OAuth host DNS resolution failed"))?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(ApiError::bad_request(
            "OAuth endpoints must resolve only to public addresses",
        ));
    }
    Ok(addresses)
}

pub(super) async fn bounded_response_body(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, ApiError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(ApiError::bad_gateway(
            "OAuth response exceeds the size limit",
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::bad_gateway("OAuth response read failed"))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(ApiError::bad_gateway(
                "OAuth response exceeds the size limit",
            ));
        }
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body)
}

pub(super) fn protected_resource_metadata_urls(resource: &Url) -> Result<Vec<Url>, ApiError> {
    let mut root = resource.clone();
    root.set_query(None);
    root.set_fragment(None);
    root.set_path("/.well-known/oauth-protected-resource");
    let resource_path = resource.path().trim_matches('/');
    let mut urls = Vec::new();
    if !resource_path.is_empty() {
        let mut path_specific = root.clone();
        path_specific
            .set_path(format!("/.well-known/oauth-protected-resource/{resource_path}").as_str());
        urls.push(path_specific);
    }
    urls.push(root);
    Ok(urls)
}

pub(super) fn authorization_server_metadata_urls(issuer: &Url) -> Result<Vec<Url>, ApiError> {
    let mut base = issuer.clone();
    base.set_query(None);
    base.set_fragment(None);
    let issuer_path = issuer.path().trim_matches('/');
    let mut oauth = base.clone();
    oauth.set_path(
        if issuer_path.is_empty() {
            "/.well-known/oauth-authorization-server".to_string()
        } else {
            format!("/.well-known/oauth-authorization-server/{issuer_path}")
        }
        .as_str(),
    );
    let mut oidc = base;
    oidc.set_path(
        if issuer_path.is_empty() {
            "/.well-known/openid-configuration".to_string()
        } else {
            format!("/{issuer_path}/.well-known/openid-configuration")
        }
        .as_str(),
    );
    Ok(vec![oauth, oidc])
}

pub(super) fn validate_public_https_url(value: &str, field: &str) -> Result<Url, ApiError> {
    let url =
        Url::parse(value).map_err(|_| ApiError::bad_request(format!("{field} is invalid")))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::bad_request(format!(
            "{field} must be a public HTTPS URL without credentials or fragment"
        )));
    }
    Ok(url)
}

pub(super) fn normalize_token_endpoint_auth_method(
    value: Option<&str>,
    has_secret: bool,
) -> Result<String, ApiError> {
    let method = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(if has_secret {
            "client_secret_basic"
        } else {
            "none"
        });
    if !matches!(
        method,
        "none" | "client_secret_basic" | "client_secret_post"
    ) {
        return Err(ApiError::bad_request(
            "OAuth token_endpoint_auth_method is unsupported",
        ));
    }
    if (method == "none") == has_secret {
        return Err(ApiError::bad_request(
            "OAuth client secret does not match token_endpoint_auth_method",
        ));
    }
    Ok(method.to_string())
}

pub(super) fn require_supported_auth_method(
    method: &str,
    supported: &[String],
) -> Result<(), ApiError> {
    if supported.is_empty() || supported.iter().any(|value| value == method) {
        Ok(())
    } else {
        Err(ApiError::conflict(format!(
            "OAuth authorization server does not support token endpoint auth method: {method}"
        )))
    }
}

pub(super) fn validate_bearer_token_type(value: Option<&str>) -> Result<(), ApiError> {
    if value.is_none_or(|value| value.eq_ignore_ascii_case("bearer")) {
        Ok(())
    } else {
        Err(ApiError::conflict(
            "OAuth token endpoint returned an unsupported token type",
        ))
    }
}

pub(super) fn normalized_token_scopes(
    returned: Option<&str>,
    requested: &[String],
) -> Result<Vec<String>, ApiError> {
    let Some(returned) = returned.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(requested.to_vec());
    };
    let scopes = normalize_scopes(
        returned
            .split_ascii_whitespace()
            .map(str::to_string)
            .collect(),
    )?;
    if scopes != requested {
        return Err(ApiError::conflict(
            "OAuth token response scopes do not exactly match the authorized signed scopes",
        ));
    }
    Ok(scopes)
}

pub(super) fn oauth_expiry(expires_in: Option<u64>) -> Result<Option<String>, ApiError> {
    let Some(seconds) = expires_in else {
        return Ok(None);
    };
    if seconds == 0 || seconds > 365 * 24 * 60 * 60 {
        return Err(ApiError::conflict("OAuth token expiry is invalid"));
    }
    let seconds =
        i64::try_from(seconds).map_err(|_| ApiError::conflict("OAuth token expiry is invalid"))?;
    Ok(Some(
        (Utc::now() + ChronoDuration::seconds(seconds)).to_rfc3339(),
    ))
}

pub(super) fn oauth_access_token_needs_refresh(
    connection: &PluginCloudOAuthConnectionRecord,
    required_valid_until_unix: i64,
) -> Result<bool, ApiError> {
    let Some(expires_at) = connection.expires_at.as_deref() else {
        return Ok(false);
    };
    let expiry = DateTime::parse_from_rfc3339(expires_at)
        .map_err(|_| ApiError::conflict("Plugin OAuth expiry is invalid"))?;
    Ok(expiry.timestamp() <= required_valid_until_unix)
}

pub(super) fn validate_token_secret(
    value: String,
    field: &str,
) -> Result<Zeroizing<String>, ApiError> {
    if value.is_empty() || value.len() > 64 * 1024 || value.chars().any(char::is_control) {
        return Err(ApiError::conflict(format!("{field} is invalid")));
    }
    Ok(Zeroizing::new(value))
}

pub(super) fn validate_oauth_text(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<String, ApiError> {
    let value = value.trim();
    if !is_bounded_oauth_text(value, max_bytes) {
        return Err(ApiError::bad_request(format!("{field} is invalid")));
    }
    Ok(value.to_string())
}

pub(super) fn is_bounded_oauth_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

pub(super) fn random_secret(bytes: usize) -> String {
    let mut secret = vec![0_u8; bytes];
    rand::fill(secret.as_mut_slice());
    URL_SAFE_NO_PAD.encode(secret)
}

pub(super) fn sha256_hex(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

pub(super) fn oauth_authorization_aad(flow_id: &str, state_sha256: &str) -> String {
    format!("chatos.plugin.cloud-oauth-authorization.v1\n{flow_id}\n{state_sha256}")
}

pub(super) fn normalized_url(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_string()
}

pub(super) fn callback_origin(public_base_url: &str) -> Result<String, ApiError> {
    let url = Url::parse(public_base_url)
        .map_err(|_| ApiError::internal("Plugin OAuth public base URL is invalid"))?;
    let origin = url.origin().ascii_serialization();
    if origin == "null" {
        return Err(ApiError::internal(
            "Plugin OAuth public base URL does not have an HTTP origin",
        ));
    }
    Ok(origin)
}

pub(super) fn oauth_callback_response(
    frontend_origin: &str,
    result: Result<PluginCloudOAuthConnectionRecord, String>,
) -> Response {
    let (ok, connection_id, message) = match result {
        Ok(connection) => (
            true,
            Some(connection.id),
            "OAuth authorization completed".to_string(),
        ),
        Err(message) => (false, None, message),
    };
    let payload = serde_json::json!({
        "type": "chatos-plugin-cloud-oauth",
        "ok": ok,
        "connection_id": connection_id,
        "message": message,
    });
    let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    let origin = serde_json::to_string(frontend_origin).unwrap_or_else(|_| "\"\"".to_string());
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>ChatOS OAuth</title></head><body><p>OAuth authorization finished. You may close this window.</p><script>if(window.opener){{window.opener.postMessage({payload},{origin});}}window.close();</script></body></html>"
    );
    let mut response = Html(html).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    response
}

pub(super) async fn write_oauth_audit(
    state: &AppState,
    event: &str,
    owner_user_id: &str,
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
    outcome: &str,
) -> Result<(), ApiError> {
    let mut audit = plugin_audit_record(
        event,
        owner_user_id,
        None,
        plugin_id,
        Some(release_id),
        outcome,
        BTreeMap::new(),
    );
    audit.component_key = Some(component_key.to_string());
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)
}

pub(super) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

pub(super) fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 224
        || ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])
        || ip.octets()[0] == 198 && matches!(ip.octets()[1], 18 | 19))
}

pub(super) fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (ip.segments()[0] & 0xfe00) == 0xfc00
        || (ip.segments()[0] & 0xffc0) == 0xfe80
        || (ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8))
}
