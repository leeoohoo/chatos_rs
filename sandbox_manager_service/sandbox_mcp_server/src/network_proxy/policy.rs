// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::address::*;
use super::*;

#[derive(Debug, Clone)]
pub(super) struct NetworkProxyPolicy {
    pub(super) mode: NetworkProxyMode,
    allow_local_binding: bool,
    pub(super) allow: Vec<DomainPattern>,
    pub(super) deny: Vec<DomainPattern>,
    #[cfg_attr(not(unix), allow(dead_code))]
    unix_sockets: BTreeMap<PathBuf, NetworkDomainPermission>,
    #[cfg_attr(not(unix), allow(dead_code))]
    dangerously_allow_all_unix_sockets: bool,
}

impl NetworkProxyPolicy {
    pub(super) fn from_requirements(requirements: &NetworkRequirements) -> Result<Self, String> {
        if requirements.allow_upstream_proxy == Some(true) {
            return Err(
                "upstream proxy chaining is not yet supported by the native ChatOS network proxy"
                    .to_string(),
            );
        }
        let mut permissions = requirements.domains.clone().unwrap_or_default();
        for host in requirements.allowed_domains.as_deref().unwrap_or_default() {
            permissions
                .entry(host.clone())
                .or_insert(NetworkDomainPermission::Allow);
        }
        for host in requirements.denied_domains.as_deref().unwrap_or_default() {
            permissions.insert(host.clone(), NetworkDomainPermission::Deny);
        }

        let mut allow = Vec::new();
        let mut deny = Vec::new();
        for (pattern, permission) in permissions {
            let pattern = DomainPattern::parse(pattern.as_str())?;
            match permission {
                NetworkDomainPermission::Allow => allow.push(pattern),
                NetworkDomainPermission::Deny => deny.push(pattern),
            }
        }
        let mut unix_sockets = BTreeMap::new();
        for (path, permission) in requirements.unix_sockets.clone().unwrap_or_default() {
            unix_sockets.insert(normalize_unix_socket_path(path.as_str())?, permission);
        }
        for path in requirements
            .allow_unix_sockets
            .as_deref()
            .unwrap_or_default()
        {
            unix_sockets
                .entry(normalize_unix_socket_path(path.as_str())?)
                .or_insert(NetworkDomainPermission::Allow);
        }
        Ok(Self {
            mode: requirements.mode.unwrap_or_default(),
            allow_local_binding: requirements.allow_local_binding.unwrap_or(false),
            allow,
            deny,
            unix_sockets,
            dangerously_allow_all_unix_sockets: requirements
                .dangerously_allow_all_unix_sockets
                .unwrap_or(false),
        })
    }

    #[cfg_attr(not(unix), allow(dead_code))]
    pub(super) fn authorize_unix_socket(&self, path: &str) -> Result<PathBuf, ProxyBlock> {
        let path = normalize_unix_socket_path(path).map_err(|detail| ProxyBlock {
            reason: "invalid-unix-socket",
            host: "unix-socket".to_string(),
            detail: Some(detail),
        })?;
        if self.unix_sockets.get(&path) == Some(&NetworkDomainPermission::Deny) {
            return Err(ProxyBlock::new(
                "blocked-by-denylist",
                path.to_string_lossy().to_string(),
            ));
        }
        if !self.dangerously_allow_all_unix_sockets
            && self.unix_sockets.get(&path) != Some(&NetworkDomainPermission::Allow)
        {
            return Err(ProxyBlock::new(
                "blocked-by-allowlist",
                path.to_string_lossy().to_string(),
            ));
        }
        Ok(path)
    }

    pub(super) async fn connect(&self, host: &str, port: u16) -> Result<TcpStream, ProxyBlock> {
        let host = normalize_host(host).map_err(|detail| ProxyBlock {
            reason: "invalid-host",
            host: host.to_string(),
            detail: Some(detail),
        })?;
        if self
            .deny
            .iter()
            .any(|pattern| pattern.matches(host.as_str()))
        {
            return Err(ProxyBlock::new("blocked-by-denylist", host));
        }
        if !self
            .allow
            .iter()
            .any(|pattern| pattern.matches(host.as_str()))
        {
            return Err(ProxyBlock::new("blocked-by-allowlist", host));
        }

        let exact_local_allow = self
            .allow
            .iter()
            .any(|pattern| pattern.is_exact_match(host.as_str()));
        let literal_ip = unscoped_ip_literal(host.as_str())
            .unwrap_or(host.as_str())
            .parse()
            .ok();
        if let Some(ip) = literal_ip {
            if is_non_public_ip(ip) && !self.allow_local_binding && !exact_local_allow {
                return Err(ProxyBlock::new("blocked-local-destination", host));
            }
            return TcpStream::connect(SocketAddr::new(ip, port))
                .await
                .map_err(|err| ProxyBlock::io(host, err));
        }
        if host == "localhost" {
            if !self.allow_local_binding && !exact_local_allow {
                return Err(ProxyBlock::new("blocked-local-destination", host));
            }
            return TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
                .await
                .map_err(|err| ProxyBlock::io(host, err));
        }

        let resolved = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|err| ProxyBlock::io(host.clone(), err))?
            .collect::<Vec<_>>();
        if resolved.is_empty() {
            return Err(ProxyBlock::new("dns-no-addresses", host));
        }
        if !self.allow_local_binding
            && resolved
                .iter()
                .any(|address| is_non_public_ip(address.ip()))
        {
            // A hostname that resolves to any local/private destination is rejected as a unit.
            // The checked SocketAddr is also used for the actual connect, so the transport cannot
            // perform a second DNS lookup after policy evaluation.
            return Err(ProxyBlock::new("blocked-local-destination", host));
        }
        let mut last_error = None;
        for address in resolved {
            match TcpStream::connect(address).await {
                Ok(stream) => return Ok(stream),
                Err(err) => last_error = Some(err),
            }
        }
        Err(ProxyBlock::io(
            host,
            last_error.unwrap_or_else(|| std::io::Error::other("connection failed")),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DomainPattern {
    AnyPublic,
    Exact(String),
    Subdomains(String),
    ApexAndSubdomains(String),
}

impl DomainPattern {
    pub(super) fn parse(pattern: &str) -> Result<Self, String> {
        let pattern = pattern.trim();
        if pattern == "*" {
            return Ok(Self::AnyPublic);
        }
        if let Some(suffix) = pattern.strip_prefix("**.") {
            return Ok(Self::ApexAndSubdomains(normalize_host(suffix)?));
        }
        if let Some(suffix) = pattern.strip_prefix("*.") {
            return Ok(Self::Subdomains(normalize_host(suffix)?));
        }
        if pattern.contains('*') {
            return Err(format!("unsupported network domain wildcard: {pattern:?}"));
        }
        Ok(Self::Exact(normalize_host(pattern)?))
    }

    pub(super) fn matches(&self, host: &str) -> bool {
        match self {
            Self::AnyPublic => true,
            Self::Exact(pattern) => host == pattern,
            Self::Subdomains(suffix) => {
                host.len() > suffix.len()
                    && host.ends_with(suffix)
                    && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
            }
            Self::ApexAndSubdomains(suffix) => {
                host == suffix
                    || (host.len() > suffix.len()
                        && host.ends_with(suffix)
                        && host.as_bytes()[host.len() - suffix.len() - 1] == b'.')
            }
        }
    }

    pub(super) fn is_exact_match(&self, host: &str) -> bool {
        matches!(self, Self::Exact(pattern) if pattern == host)
    }
}

#[derive(Debug)]
pub(super) struct ProxyBlock {
    pub(super) reason: &'static str,
    pub(super) host: String,
    pub(super) detail: Option<String>,
}

impl ProxyBlock {
    pub(super) fn new(reason: &'static str, host: String) -> Self {
        Self {
            reason,
            host,
            detail: None,
        }
    }

    pub(super) fn io(host: String, error: impl std::fmt::Display) -> Self {
        Self {
            reason: "upstream-connection-failed",
            host,
            detail: Some(error.to_string()),
        }
    }
}
