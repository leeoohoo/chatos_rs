// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;

use super::{
    MAX_CONFIGURED_HEADERS, MAX_CONFIGURED_HEADER_BYTES, MAX_TOOL_NAME_BYTES, MAX_TOOL_POLICY_ITEMS,
};

pub(super) fn validate_plugin_resolved_headers(
    templates: &BTreeMap<String, String>,
    resolved: &BTreeMap<String, String>,
    oauth_enabled: bool,
    permissions: &[String],
) -> Result<(), String> {
    let mut expected = BTreeSet::new();
    let mut uses_credentials = false;
    for (name, template) in templates {
        let normalized = name.trim().to_ascii_lowercase();
        HeaderName::from_bytes(normalized.as_bytes())
            .map_err(|_| format!("Plugin HTTP header is invalid: {normalized}"))?;
        let uses_template_credential = template.contains("${credential:");
        if !uses_template_credential
            && !matches!(
                normalized.as_str(),
                "accept"
                    | "accept-language"
                    | "content-type"
                    | "mcp-protocol-version"
                    | "user-agent"
                    | "x-plugin-client"
            )
        {
            return Err(format!(
                "Plugin HTTP custom header must use a credential template: {normalized}"
            ));
        }
        uses_credentials |= uses_template_credential;
        expected.insert(normalized);
    }
    if oauth_enabled {
        if expected.contains("authorization") {
            return Err(
                "Plugin HTTP MCP cannot combine OAuth with an Authorization template".to_string(),
            );
        }
        expected.insert("authorization".to_string());
    }
    let actual = resolved
        .keys()
        .map(|name| name.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(
            "Plugin HTTP resolved headers do not match the immutable templates".to_string(),
        );
    }
    if uses_credentials
        && !permissions.iter().any(|permission| {
            permission == "credential.use" || permission.starts_with("credential.use:")
        })
    {
        return Err(
            "Plugin HTTP credentials require credential.use in the immutable permission snapshot"
                .to_string(),
        );
    }
    Ok(())
}

pub(super) fn validate_endpoint(value: &str) -> Result<reqwest::Url, String> {
    let endpoint =
        reqwest::Url::parse(value.trim()).map_err(|_| "endpoint URL is invalid".to_string())?;
    validate_endpoint_url(&endpoint)?;
    Ok(endpoint)
}

fn validate_endpoint_url(endpoint: &reqwest::Url) -> Result<(), String> {
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("endpoint must use HTTPS without URL credentials or fragments".to_string());
    }
    Ok(())
}

pub(crate) fn build_pinned_external_http_client(
    endpoint: &reqwest::Url,
    addresses: &[SocketAddr],
    request_timeout: Duration,
) -> Result<reqwest::Client, String> {
    validate_endpoint_url(endpoint)?;
    let host = endpoint
        .host_str()
        .ok_or_else(|| "endpoint has no host".to_string())?;
    let port = endpoint
        .port_or_known_default()
        .ok_or_else(|| "endpoint has no usable port".to_string())?;
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| address.port() != port || !is_public_ip(address.ip()))
    {
        return Err(
            "endpoint must remain pinned only to public addresses on its configured port"
                .to_string(),
        );
    }
    reqwest::Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10).min(request_timeout))
        .timeout(request_timeout)
        .resolve_to_addrs(host, addresses)
        .build()
        .map_err(|_| "build endpoint client failed".to_string())
}

pub(super) fn configured_headers(
    configured: &BTreeMap<String, String>,
) -> Result<HeaderMap, String> {
    if configured.len() > MAX_CONFIGURED_HEADERS {
        return Err(format!(
            "headers exceed the supported {MAX_CONFIGURED_HEADERS} entries"
        ));
    }
    let encoded_bytes = configured
        .iter()
        .map(|(name, value)| name.len().saturating_add(value.len()))
        .sum::<usize>();
    if encoded_bytes > MAX_CONFIGURED_HEADER_BYTES {
        return Err(format!(
            "headers exceed the supported {MAX_CONFIGURED_HEADER_BYTES} bytes"
        ));
    }
    let mut headers = HeaderMap::new();
    for (name, value) in configured {
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| "headers contain an invalid name".to_string())?;
        if header_is_managed_or_unsafe(&name) {
            return Err(format!(
                "header {} is managed by MCP Management and cannot be configured",
                name.as_str()
            ));
        }
        let mut value = HeaderValue::from_str(value)
            .map_err(|_| "headers contain an invalid value".to_string())?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

pub(crate) fn header_is_managed_or_unsafe(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "accept"
            | "connection"
            | "content-length"
            | "content-type"
            | "host"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "x-local-connector-internal-scope"
            | "x-local-connector-internal-secret"
            | "x-local-connector-internal-token"
            | "x-project-service-internal-scope"
            | "x-project-service-internal-token"
            | "x-project-service-sync-secret"
            | "x-sandbox-client-key"
            | "x-sandbox-internal-scope"
            | "x-sandbox-internal-token"
    )
}

pub(super) fn configured_tool_names(
    values: &[String],
    field: &str,
) -> Result<HashSet<String>, String> {
    if values.len() > MAX_TOOL_POLICY_ITEMS {
        return Err(format!(
            "{field} exceeds the supported {MAX_TOOL_POLICY_ITEMS} entries"
        ));
    }
    let mut normalized = HashSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || value.len() > MAX_TOOL_NAME_BYTES {
            return Err(format!("{field} contains an invalid tool name"));
        }
        normalized.insert(value.to_string());
    }
    Ok(normalized)
}

pub(super) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}
