// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::providers::{
    build_pinned_external_http_client, external_http_header_is_managed_or_unsafe,
};

use super::{
    ExternalHttpProviderBinding, MAX_PERSISTED_HEADERS, MAX_PERSISTED_HEADER_BYTES,
    MAX_PERSISTED_TOOL_NAME_BYTES, MAX_PERSISTED_TOOL_POLICY_ITEMS,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedExternalHttpProviderBinding {
    pub(super) provider_ref: String,
    pub(super) endpoint: String,
    pub(super) headers: Vec<PersistedHeader>,
    pub(super) resolved_addresses: Vec<String>,
    pub(super) allow_writes: bool,
    pub(super) allowed_tool_names: HashSet<String>,
    pub(super) blocked_tool_names: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedHeader {
    pub(super) name: String,
    pub(super) value: Vec<u8>,
}

pub(super) fn persist_external_http_binding(
    binding: &ExternalHttpProviderBinding,
) -> Result<PersistedExternalHttpProviderBinding, String> {
    let headers = binding
        .headers
        .iter()
        .map(|(name, value)| PersistedHeader {
            name: name.as_str().to_string(),
            value: value.as_bytes().to_vec(),
        })
        .collect::<Vec<_>>();
    validate_persisted_headers(headers.as_slice())?;
    validate_persisted_tool_names(&binding.allowed_tool_names, "allowed_tool_names")?;
    validate_persisted_tool_names(&binding.blocked_tool_names, "blocked_tool_names")?;
    Ok(PersistedExternalHttpProviderBinding {
        provider_ref: binding.provider_ref.clone(),
        endpoint: binding.endpoint.as_str().to_string(),
        headers,
        resolved_addresses: binding
            .resolved_addresses
            .iter()
            .map(ToString::to_string)
            .collect(),
        allow_writes: binding.allow_writes,
        allowed_tool_names: binding.allowed_tool_names.clone(),
        blocked_tool_names: binding.blocked_tool_names.clone(),
    })
}

pub(super) fn restore_external_http_binding(
    persisted: PersistedExternalHttpProviderBinding,
    request_timeout: Duration,
) -> Result<ExternalHttpProviderBinding, String> {
    if persisted.provider_ref.trim().is_empty() {
        return Err("persisted External HTTP Provider reference is empty".to_string());
    }
    validate_persisted_headers(persisted.headers.as_slice())?;
    validate_persisted_tool_names(&persisted.allowed_tool_names, "allowed_tool_names")?;
    validate_persisted_tool_names(&persisted.blocked_tool_names, "blocked_tool_names")?;
    let endpoint = reqwest::Url::parse(persisted.endpoint.trim())
        .map_err(|_| "persisted External HTTP endpoint is invalid".to_string())?;
    let resolved_addresses = persisted
        .resolved_addresses
        .iter()
        .map(|value| {
            value
                .parse::<SocketAddr>()
                .map_err(|_| "persisted External HTTP address is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let http = build_pinned_external_http_client(
        &endpoint,
        resolved_addresses.as_slice(),
        request_timeout,
    )?;
    let mut headers = HeaderMap::new();
    for persisted_header in persisted.headers {
        let name = HeaderName::from_bytes(persisted_header.name.as_bytes())
            .map_err(|_| "persisted External HTTP header name is invalid".to_string())?;
        if external_http_header_is_managed_or_unsafe(&name) {
            return Err("persisted External HTTP header is managed or unsafe".to_string());
        }
        let mut value = HeaderValue::from_bytes(persisted_header.value.as_slice())
            .map_err(|_| "persisted External HTTP header value is invalid".to_string())?;
        value.set_sensitive(true);
        headers.append(name, value);
    }
    Ok(ExternalHttpProviderBinding {
        provider_ref: persisted.provider_ref,
        endpoint,
        headers,
        http,
        resolved_addresses,
        allow_writes: persisted.allow_writes,
        allowed_tool_names: persisted.allowed_tool_names,
        blocked_tool_names: persisted.blocked_tool_names,
    })
}

fn validate_persisted_headers(headers: &[PersistedHeader]) -> Result<(), String> {
    if headers.len() > MAX_PERSISTED_HEADERS {
        return Err("persisted External HTTP headers exceed the supported limit".to_string());
    }
    let bytes = headers.iter().fold(0_usize, |total, header| {
        total
            .saturating_add(header.name.len())
            .saturating_add(header.value.len())
    });
    if bytes > MAX_PERSISTED_HEADER_BYTES {
        return Err("persisted External HTTP headers exceed the supported size".to_string());
    }
    Ok(())
}

fn validate_persisted_tool_names(values: &HashSet<String>, field: &str) -> Result<(), String> {
    if values.len() > MAX_PERSISTED_TOOL_POLICY_ITEMS
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > MAX_PERSISTED_TOOL_NAME_BYTES)
    {
        return Err(format!("persisted External HTTP {field} is invalid"));
    }
    Ok(())
}
