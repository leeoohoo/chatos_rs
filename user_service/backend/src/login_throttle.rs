// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, SocketAddr};

use axum::http::HeaderMap;
use chatos_postgres::PgPool;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct LoginThrottle {
    pool: PgPool,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoginFailureRecord {
    key: String,
    attempts: i64,
    window_start_unix: i64,
    locked_until_unix: Option<i64>,
    expires_at_unix: i64,
}

impl LoginThrottle {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn is_locked(
        &self,
        username: &str,
        source: Option<&str>,
        now_unix: i64,
        config: &AppConfig,
    ) -> Result<bool, String> {
        if config.login_max_failed_attempts <= 0 {
            return Ok(false);
        }
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM login_throttle
                WHERE key = ANY($1) AND locked_until_unix > $2 AND expires_at > now()
            )
            "#,
        )
        .bind(throttle_keys(username, source))
        .bind(now_unix)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| err.to_string())
    }

    pub async fn record_failure(
        &self,
        username: &str,
        source: Option<&str>,
        now_unix: i64,
        config: &AppConfig,
    ) -> Result<(), String> {
        if config.login_max_failed_attempts <= 0 {
            return Ok(());
        }
        let retention_seconds = config
            .login_failure_window_seconds
            .max(config.login_lockout_seconds)
            .max(60);
        for key in throttle_keys(username, source) {
            sqlx::query(
                r#"
                INSERT INTO login_throttle
                    (key, attempts, window_start_unix, locked_until_unix, expires_at)
                VALUES (
                    $1,
                    1,
                    $2,
                    CASE WHEN 1 >= $4 THEN $2 + $5 ELSE NULL END,
                    to_timestamp($2 + $6)
                )
                ON CONFLICT (key) DO UPDATE SET
                    attempts = CASE
                        WHEN $2 - login_throttle.window_start_unix >= $3 THEN 1
                        ELSE login_throttle.attempts + 1
                    END,
                    window_start_unix = CASE
                        WHEN $2 - login_throttle.window_start_unix >= $3 THEN $2
                        ELSE login_throttle.window_start_unix
                    END,
                    locked_until_unix = CASE
                        WHEN (
                            CASE
                                WHEN $2 - login_throttle.window_start_unix >= $3 THEN 1
                                ELSE login_throttle.attempts + 1
                            END
                        ) >= $4 THEN $2 + $5
                        ELSE NULL
                    END,
                    expires_at = to_timestamp($2 + $6)
                "#,
            )
            .bind(key)
            .bind(now_unix)
            .bind(config.login_failure_window_seconds)
            .bind(config.login_max_failed_attempts)
            .bind(config.login_lockout_seconds)
            .bind(retention_seconds)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    pub async fn record_success(&self, username: &str, source: Option<&str>) -> Result<(), String> {
        sqlx::query("DELETE FROM login_throttle WHERE key = ANY($1)")
            .bind(throttle_keys(username, source))
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[cfg(test)]
fn next_failure_record(
    existing: Option<&LoginFailureRecord>,
    key: &str,
    now_unix: i64,
    max_failed_attempts: i64,
    failure_window_seconds: i64,
    lockout_seconds: i64,
) -> LoginFailureRecord {
    let window_expired = existing.is_none_or(|record| {
        now_unix.saturating_sub(record.window_start_unix) >= failure_window_seconds
    });
    let window_start_unix = if window_expired {
        now_unix
    } else {
        existing.map_or(now_unix, |record| record.window_start_unix)
    };
    let attempts = if window_expired {
        1
    } else {
        existing.map_or(1, |record| record.attempts.saturating_add(1))
    };
    let locked_until_unix =
        (attempts >= max_failed_attempts).then_some(now_unix.saturating_add(lockout_seconds));
    let retention_seconds = failure_window_seconds.max(lockout_seconds).max(60);
    LoginFailureRecord {
        key: key.to_string(),
        attempts,
        window_start_unix,
        locked_until_unix,
        expires_at_unix: now_unix.saturating_add(retention_seconds),
    }
}

fn throttle_keys(username: &str, source: Option<&str>) -> Vec<String> {
    let mut keys = vec![format!("username:{}", username.trim())];
    if let Some(source) = source.map(str::trim).filter(|value| !value.is_empty()) {
        keys.push(format!("source:{source}"));
    }
    keys
}

pub fn request_source(headers: &HeaderMap, peer: SocketAddr) -> String {
    let peer_ip = peer.ip();
    if is_trusted_proxy_address(peer_ip) {
        if let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<IpAddr>().ok())
        {
            return forwarded.to_string();
        }
    }
    peer_ip.to_string()
}

fn is_trusted_proxy_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_keys_cover_identity_and_source() {
        assert_eq!(
            throttle_keys(" alice ", Some(" 203.0.113.1 ")),
            vec!["username:alice", "source:203.0.113.1"]
        );
    }

    #[test]
    fn failure_transition_locks_and_resets_after_window() {
        let first = next_failure_record(None, "username:alice", 100, 2, 10, 30);
        let second = next_failure_record(Some(&first), "username:alice", 101, 2, 10, 30);
        assert_eq!(second.attempts, 2);
        assert_eq!(second.locked_until_unix, Some(131));
        let reset = next_failure_record(Some(&second), "username:alice", 111, 2, 10, 30);
        assert_eq!(reset.attempts, 1);
        assert_eq!(reset.locked_until_unix, None);
    }

    #[test]
    fn forwarded_address_is_only_used_from_private_proxy_peer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.8".parse().expect("header"));
        let private_peer = SocketAddr::from(([10, 0, 0, 2], 4000));
        let public_peer = SocketAddr::from(([8, 8, 8, 8], 4000));
        assert_eq!(request_source(&headers, private_peer), "203.0.113.8");
        assert_eq!(request_source(&headers, public_peer), "8.8.8.8");
    }
}
