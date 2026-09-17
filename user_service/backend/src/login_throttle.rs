// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use axum::http::HeaderMap;
use mongodb::bson::{doc, DateTime};
use mongodb::options::{IndexOptions, UpdateOptions};
use mongodb::{Collection, IndexModel};
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;

#[derive(Clone)]
pub struct LoginThrottle {
    records: Collection<LoginFailureRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LoginFailureRecord {
    #[serde(rename = "_id")]
    key: String,
    attempts: i64,
    window_start_unix: i64,
    locked_until_unix: Option<i64>,
    expires_at: DateTime,
}

impl LoginThrottle {
    pub(crate) fn new(records: Collection<LoginFailureRecord>) -> Self {
        Self { records }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.records
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "expires_at": 1 })
                    .options(
                        IndexOptions::builder()
                            .expire_after(Some(Duration::ZERO))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|err| format!("create login throttle TTL index failed: {err}"))?;
        Ok(())
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
        self.records
            .find_one(
                doc! {
                    "_id": { "$in": throttle_keys(username, source) },
                    "locked_until_unix": { "$gt": now_unix },
                },
                None,
            )
            .await
            .map(|record| record.is_some())
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
        for key in throttle_keys(username, source) {
            self.record_failure_for_key(key, now_unix, config).await?;
        }
        Ok(())
    }

    async fn record_failure_for_key(
        &self,
        key: String,
        now_unix: i64,
        config: &AppConfig,
    ) -> Result<(), String> {
        for _ in 0..8 {
            let existing = self
                .records
                .find_one(doc! { "_id": &key }, None)
                .await
                .map_err(|err| err.to_string())?;
            let next = next_failure_record(existing.as_ref(), key.as_str(), now_unix, config);
            let filter = match existing.as_ref() {
                Some(record) => doc! {
                    "_id": &key,
                    "attempts": record.attempts,
                    "window_start_unix": record.window_start_unix,
                    "locked_until_unix": record.locked_until_unix,
                },
                None => doc! { "_id": &key, "attempts": { "$exists": false } },
            };
            let update = doc! {
                "$set": {
                    "attempts": next.attempts,
                    "window_start_unix": next.window_start_unix,
                    "locked_until_unix": next.locked_until_unix,
                    "expires_at": next.expires_at,
                },
                "$setOnInsert": { "_id": &key },
            };
            let result = self
                .records
                .update_one(
                    filter,
                    update,
                    UpdateOptions::builder().upsert(existing.is_none()).build(),
                )
                .await;
            match result {
                Ok(result) if result.matched_count == 1 || result.upserted_id.is_some() => {
                    return Ok(())
                }
                Ok(_) => continue,
                Err(error) if error.to_string().contains("E11000") => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("login throttle update was contended too many times".to_string())
    }

    pub async fn record_success(&self, username: &str, source: Option<&str>) -> Result<(), String> {
        self.records
            .delete_many(
                doc! { "_id": { "$in": throttle_keys(username, source) } },
                None,
            )
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

fn next_failure_record(
    existing: Option<&LoginFailureRecord>,
    key: &str,
    now_unix: i64,
    config: &AppConfig,
) -> LoginFailureRecord {
    let window_expired = existing.is_none_or(|record| {
        now_unix.saturating_sub(record.window_start_unix) >= config.login_failure_window_seconds
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
    let locked_until_unix = (attempts >= config.login_max_failed_attempts)
        .then_some(now_unix.saturating_add(config.login_lockout_seconds));
    let retention_seconds = config
        .login_failure_window_seconds
        .max(config.login_lockout_seconds)
        .max(60);
    LoginFailureRecord {
        key: key.to_string(),
        attempts,
        window_start_unix,
        locked_until_unix,
        expires_at: DateTime::from_millis(
            now_unix
                .saturating_add(retention_seconds)
                .saturating_mul(1000),
        ),
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
    use super::{next_failure_record, request_source, throttle_keys};
    use crate::config::AppConfig;
    use axum::http::HeaderMap;
    use std::net::SocketAddr;

    fn config() -> AppConfig {
        AppConfig {
            host: "127.0.0.1".parse().unwrap(),
            port: 39190,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 1.0,
            otlp_export_timeout: std::time::Duration::from_secs(1),
            database_url: "mongodb://127.0.0.1:27017/test".to_string(),
            mongodb_database: "test".to_string(),
            jwt_secret: "test-secret".to_string(),
            jwt_issuer: "user_service".to_string(),
            user_service_audience: "user_service".to_string(),
            task_runner_audience: "task_runner".to_string(),
            user_access_ttl_seconds: 3600,
            task_runner_access_ttl_seconds: 3600,
            super_admin_username: "admin".to_string(),
            super_admin_password: "password".to_string(),
            super_admin_display_name: "Admin".to_string(),
            memory_engine_internal_api_secret: None,
            task_runner_internal_api_secret: None,
            downstream_request_timeout_ms: 5000,
            harness_provisioning_enabled: false,
            harness_base_url: None,
            harness_synthetic_email_domain: "chatos.local".to_string(),
            harness_space_prefix: "u-".to_string(),
            harness_request_timeout_ms: 5000,
            harness_project_pat_prefix: "chatos-project".to_string(),
            chatos_internal_api_secret: None,
            smtp_host: None,
            smtp_port: 587,
            smtp_username: None,
            smtp_password: None,
            email_from: None,
            email_from_name: "Chat OS".to_string(),
            registration_code_ttl_seconds: 600,
            registration_code_resend_seconds: 60,
            registration_code_hourly_limit: 5,
            registration_code_max_attempts: 5,
            login_max_failed_attempts: 3,
            login_failure_window_seconds: 300,
            login_lockout_seconds: 120,
            wechat_mini_program_app_id: None,
            wechat_mini_program_app_secret: None,
            wechat_mini_program_identity_hash_secret: None,
            wechat_mini_program_api_base_url: "https://api.weixin.qq.com".to_string(),
            wechat_mini_program_env_version: "release".to_string(),
            wechat_mini_program_development_login_enabled: false,
            wechat_mini_program_request_timeout_ms: 5_000,
            wechat_mini_program_bind_ticket_ttl_seconds: 120,
            wechat_mini_program_client_session_ttl_seconds: 604_800,
        }
    }

    #[test]
    fn failure_transition_locks_and_resets_after_window() {
        let config = config();
        let first = next_failure_record(None, "username:admin", 1000, &config);
        let second = next_failure_record(Some(&first), "username:admin", 1001, &config);
        let third = next_failure_record(Some(&second), "username:admin", 1002, &config);
        assert_eq!(third.locked_until_unix, Some(1122));

        let reset = next_failure_record(Some(&third), "username:admin", 1300, &config);
        assert_eq!(reset.attempts, 1);
        assert_eq!(reset.locked_until_unix, None);
    }

    #[test]
    fn throttle_keys_cover_identity_and_source() {
        assert_eq!(
            throttle_keys(" admin ", Some("203.0.113.5")),
            ["username:admin", "source:203.0.113.5"]
        );
    }

    #[test]
    fn forwarded_address_is_only_used_from_private_proxy_peer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.8, 10.0.0.2".parse().unwrap());
        let proxy: SocketAddr = "10.0.0.2:1234".parse().unwrap();
        let public_peer: SocketAddr = "198.51.100.2:1234".parse().unwrap();
        assert_eq!(request_source(&headers, proxy), "203.0.113.8");
        assert_eq!(request_source(&headers, public_peer), "198.51.100.2");
    }
}
