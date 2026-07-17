// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn parse_authority(authority: &str, default_port: u16) -> Result<(String, u16), String> {
    let candidate = if authority.contains("://") {
        authority.to_string()
    } else {
        format!("http://{authority}")
    };
    let url = Url::parse(candidate.as_str())
        .map_err(|_| format!("invalid network authority: {authority:?}"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!("invalid network authority: {authority:?}"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("network authority has no host: {authority:?}"))?;
    Ok((host.to_string(), url.port().unwrap_or(default_port)))
}

pub(super) fn normalize_host(input: &str) -> Result<String, String> {
    let input = input.trim().trim_end_matches('.');
    if input.is_empty() || input.contains('\0') || input.contains('/') || input.contains('@') {
        return Err(format!("invalid network host: {input:?}"));
    }
    let unscoped = unscoped_ip_literal(input).unwrap_or(input);
    if let Ok(ip) = unscoped.parse::<IpAddr>() {
        return Ok(ip.to_string().to_ascii_lowercase());
    }
    Host::parse(unscoped)
        .map(|host| host.to_string().to_ascii_lowercase())
        .map_err(|_| format!("invalid network host: {input:?}"))
}

pub(super) fn normalize_unix_socket_path(input: &str) -> Result<PathBuf, String> {
    let path = Path::new(input.trim());
    if !path.is_absolute() || input.contains('\0') {
        return Err("unix socket paths must be absolute and must not contain NUL".to_string());
    }
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|err| format!("canonicalize unix socket path failed: {err}"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "unix socket path has no parent".to_string())?;
    let parent = parent
        .canonicalize()
        .map_err(|err| format!("canonicalize unix socket parent failed: {err}"))?;
    let name = path
        .file_name()
        .ok_or_else(|| "unix socket path has no filename".to_string())?;
    Ok(parent.join(name))
}

pub(super) fn unscoped_ip_literal(host: &str) -> Option<&str> {
    host.strip_prefix('[')?.strip_suffix(']')
}

pub(super) fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ipv4_in_cidr(ip, [0, 0, 0, 0], 8)
                || ipv4_in_cidr(ip, [100, 64, 0, 0], 10)
                || ipv4_in_cidr(ip, [192, 0, 0, 0], 24)
                || ipv4_in_cidr(ip, [192, 0, 2, 0], 24)
                || ipv4_in_cidr(ip, [198, 18, 0, 0], 15)
                || ipv4_in_cidr(ip, [198, 51, 100, 0], 24)
                || ipv4_in_cidr(ip, [203, 0, 113, 0], 24)
                || ipv4_in_cidr(ip, [240, 0, 0, 0], 4)
        }
        IpAddr::V6(ip) => {
            if let Some(ipv4) = ip.to_ipv4_mapped() {
                return is_non_public_ip(IpAddr::V4(ipv4));
            }
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
                || (ip.segments()[0] & 0xffc0) == 0xfec0
                || (ip.segments()[0] & 0xffc0) == 0x0000
        }
    }
}

pub(super) fn ipv4_in_cidr(ip: Ipv4Addr, base: [u8; 4], prefix: u8) -> bool {
    let ip = u32::from(ip);
    let base = u32::from(Ipv4Addr::from(base));
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (ip & mask) == (base & mask)
}
