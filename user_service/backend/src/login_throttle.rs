// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::AppConfig;

#[derive(Debug, Clone, Default)]
pub struct LoginThrottle {
    records: Arc<Mutex<HashMap<String, LoginFailureRecord>>>,
}

#[derive(Debug, Clone)]
struct LoginFailureRecord {
    attempts: i64,
    window_start_unix: i64,
    locked_until_unix: Option<i64>,
}

impl LoginThrottle {
    pub fn is_locked(
        &self,
        username: &str,
        source: Option<&str>,
        now_unix: i64,
        config: &AppConfig,
    ) -> bool {
        if config.login_max_failed_attempts <= 0 {
            return false;
        }

        let mut records = self.records.lock().expect("login throttle mutex poisoned");
        for key in throttle_keys(username, source) {
            let Some(record) = records.get(key.as_str()) else {
                continue;
            };

            if record
                .locked_until_unix
                .is_some_and(|locked_until| locked_until > now_unix)
            {
                return true;
            }

            if now_unix - record.window_start_unix >= config.login_failure_window_seconds {
                records.remove(key.as_str());
            }
        }
        false
    }

    pub fn record_failure(
        &self,
        username: &str,
        source: Option<&str>,
        now_unix: i64,
        config: &AppConfig,
    ) {
        if config.login_max_failed_attempts <= 0 {
            return;
        }

        let mut records = self.records.lock().expect("login throttle mutex poisoned");
        for key in throttle_keys(username, source) {
            let record = records.entry(key).or_insert_with(|| LoginFailureRecord {
                attempts: 0,
                window_start_unix: now_unix,
                locked_until_unix: None,
            });

            if now_unix - record.window_start_unix >= config.login_failure_window_seconds {
                record.attempts = 0;
                record.window_start_unix = now_unix;
                record.locked_until_unix = None;
            }

            record.attempts += 1;
            if record.attempts >= config.login_max_failed_attempts {
                record.locked_until_unix = Some(now_unix + config.login_lockout_seconds);
            }
        }
    }

    pub fn record_success(&self, username: &str, source: Option<&str>) {
        let mut records = self.records.lock().expect("login throttle mutex poisoned");
        for key in throttle_keys(username, source) {
            records.remove(key.as_str());
        }
    }
}

fn throttle_keys(username: &str, source: Option<&str>) -> Vec<String> {
    let mut keys = vec![format!("username:{}", username.trim())];
    if let Some(source) = source.map(str::trim).filter(|value| !value.is_empty()) {
        keys.push(format!("source:{source}"));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::LoginThrottle;
    use crate::config::AppConfig;

    fn config() -> AppConfig {
        AppConfig {
            host: "127.0.0.1".parse().unwrap(),
            port: 39190,
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
            memory_engine_base_url: None,
            memory_engine_operator_token: None,
            task_runner_base_url: None,
            task_runner_callback_secret: None,
            downstream_request_timeout_ms: 5000,
            harness_provisioning_enabled: false,
            harness_base_url: None,
            harness_synthetic_email_domain: "chatos.local".to_string(),
            harness_space_prefix: "u-".to_string(),
            harness_request_timeout_ms: 5000,
            harness_project_pat_prefix: "chatos-project".to_string(),
            user_service_internal_api_secret: None,
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
        }
    }

    #[test]
    fn locks_after_configured_failures() {
        let throttle = LoginThrottle::default();
        let config = config();
        assert!(!throttle.is_locked("admin", None, 1000, &config));
        throttle.record_failure("admin", None, 1000, &config);
        throttle.record_failure("admin", None, 1001, &config);
        assert!(!throttle.is_locked("admin", None, 1002, &config));
        throttle.record_failure("admin", None, 1002, &config);
        assert!(throttle.is_locked("admin", None, 1003, &config));
    }

    #[test]
    fn unlocks_after_lockout_expires() {
        let throttle = LoginThrottle::default();
        let config = config();
        throttle.record_failure("admin", None, 1000, &config);
        throttle.record_failure("admin", None, 1001, &config);
        throttle.record_failure("admin", None, 1002, &config);
        assert!(throttle.is_locked("admin", None, 1003, &config));
        assert!(!throttle.is_locked("admin", None, 1123, &config));
    }

    #[test]
    fn success_clears_failures() {
        let throttle = LoginThrottle::default();
        let config = config();
        throttle.record_failure("admin", Some("127.0.0.1"), 1000, &config);
        throttle.record_failure("admin", Some("127.0.0.1"), 1001, &config);
        throttle.record_success("admin", Some("127.0.0.1"));
        throttle.record_failure("admin", Some("127.0.0.1"), 1002, &config);
        assert!(!throttle.is_locked("admin", Some("127.0.0.1"), 1003, &config));
    }

    #[test]
    fn locks_source_after_failures_for_different_usernames() {
        let throttle = LoginThrottle::default();
        let config = config();
        throttle.record_failure("first", Some("127.0.0.1"), 1000, &config);
        throttle.record_failure("second", Some("127.0.0.1"), 1001, &config);
        assert!(!throttle.is_locked("third", Some("127.0.0.1"), 1002, &config));
        throttle.record_failure("third", Some("127.0.0.1"), 1002, &config);

        assert!(throttle.is_locked("fourth", Some("127.0.0.1"), 1003, &config));
        assert!(!throttle.is_locked("fourth", Some("127.0.0.2"), 1003, &config));
    }
}
