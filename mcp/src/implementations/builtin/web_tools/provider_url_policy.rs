// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use reqwest::Client;
use url::Url;

pub(crate) const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

pub fn build_web_client(timeout: Duration, user_agent: &str) -> Result<Client, String> {
    Client::builder()
        .timeout(timeout)
        .user_agent(user_agent)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "too many redirects",
                ));
            }

            if ensure_public_web_url(attempt.url()).is_ok() {
                return attempt.follow();
            }

            let blocked_url = attempt.url().to_string();
            attempt.error(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("blocked unsafe redirect target: {}", blocked_url),
            ))
        }))
        .build()
        .map_err(|err| format!("build web client failed: {}", err))
}

pub fn normalize_public_web_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parsed = Url::parse(trimmed).ok()?;
    ensure_public_web_url(&parsed).ok()?;
    parsed.set_fragment(None);
    Some(parsed.to_string())
}

pub(crate) fn resolve_public_web_url(raw: &str, base_url: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed =
        Url::parse(trimmed).or_else(|_| Url::parse(base_url).and_then(|base| base.join(trimmed)));
    let parsed = parsed.ok()?;

    if let Some(target) = parsed.query_pairs().find_map(|(key, value)| {
        if key == "uddg" {
            Some(value.into_owned())
        } else {
            None
        }
    }) {
        return normalize_public_web_url(target.as_str());
    }

    normalize_public_web_url(parsed.as_str())
}

fn ensure_public_web_url(url: &Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http(s) URLs are allowed".to_string());
    }

    let host = url
        .host_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "URL host is missing".to_string())?;
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err("URL host is missing".to_string());
    }

    if is_blocked_hostname(host.as_str()) {
        return Err(format!("host is not publicly routable: {}", host));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_forbidden_ip(ip) {
            return Err(format!("IP address is not publicly routable: {}", host));
        }
    }

    Ok(())
}

fn is_blocked_hostname(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "local"
        || host.ends_with(".local")
        || host.ends_with(".localdomain")
        || host == "internal"
        || host.ends_with(".internal")
        || host == "home.arpa"
        || host.ends_with(".home.arpa")
        || !host.contains('.')
}

fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => is_forbidden_ipv4(addr),
        IpAddr::V6(addr) => is_forbidden_ipv6(addr),
    }
}

fn is_forbidden_ipv4(addr: Ipv4Addr) -> bool {
    let [a, b, c, _] = addr.octets();

    addr.is_unspecified()
        || addr.is_private()
        || addr.is_loopback()
        || addr.is_link_local()
        || addr.is_broadcast()
        || addr.is_documentation()
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 224
}

fn is_forbidden_ipv6(addr: Ipv6Addr) -> bool {
    if addr.is_unspecified() || addr.is_loopback() || addr.is_multicast() {
        return true;
    }

    if addr.to_ipv4_mapped().is_some_and(is_forbidden_ipv4) {
        return true;
    }

    let segments = addr.segments();
    let first = segments[0];

    (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
}

#[cfg(test)]
mod tests {
    use super::{normalize_public_web_url, resolve_public_web_url};

    #[test]
    fn normalize_public_web_url_allows_public_https_and_strips_fragments() {
        assert_eq!(
            normalize_public_web_url("https://example.com/docs/page#section"),
            Some("https://example.com/docs/page".to_string())
        );
    }

    #[test]
    fn normalize_public_web_url_blocks_non_public_targets() {
        assert!(normalize_public_web_url("ftp://example.com/file.txt").is_none());
        assert!(normalize_public_web_url("http://127.0.0.1:8080/health").is_none());
        assert!(normalize_public_web_url("http://localhost:3000/app").is_none());
        assert!(normalize_public_web_url("http://[::1]/").is_none());
        assert!(normalize_public_web_url("http://169.254.169.254/latest/meta-data").is_none());
        assert!(normalize_public_web_url("http://10.0.0.5/internal").is_none());
        assert!(normalize_public_web_url("http://192.168.1.10/panel").is_none());
        assert!(normalize_public_web_url("http://example").is_none());
        assert!(normalize_public_web_url("http://printer.local/status").is_none());
        assert!(normalize_public_web_url("http://service.internal/api").is_none());
    }

    #[test]
    fn normalize_public_web_url_blocks_additional_non_public_targets() {
        assert!(normalize_public_web_url("http://nas.localdomain/files").is_none());
        assert!(normalize_public_web_url("http://router.home.arpa/status").is_none());
        assert!(normalize_public_web_url("http://100.64.0.8/private").is_none());
        assert!(normalize_public_web_url("http://[::ffff:127.0.0.1]/admin").is_none());
        assert!(normalize_public_web_url("http://[::ffff:10.0.0.8]/internal").is_none());
    }

    #[test]
    fn resolve_public_web_url_rejects_private_redirect_targets() {
        assert_eq!(
            resolve_public_web_url(
                "https://duckduckgo.com/l/?uddg=http%3A%2F%2F127.0.0.1%3A8080%2Fadmin",
                "https://duckduckgo.com"
            ),
            None
        );
    }

    #[test]
    fn resolve_public_web_url_rejects_relative_private_target() {
        assert_eq!(
            resolve_public_web_url("/admin", "http://127.0.0.1:8080"),
            None
        );
    }

    #[test]
    fn normalize_public_web_url_blocks_private_ipv6_ranges() {
        assert!(normalize_public_web_url("http://[fc00::1]/").is_none());
        assert!(normalize_public_web_url("http://[fd12:3456:789a::1]/").is_none());
        assert!(normalize_public_web_url("http://[fe80::1]/").is_none());
    }
}
