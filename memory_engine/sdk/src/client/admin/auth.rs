// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use crate::models::SdkAuthStatusResponse;

use super::super::transport::append_optional_query;
use super::super::{AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn get_sdk_auth_status(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<SdkAuthStatusResponse, String> {
        match &self.auth {
            AuthMode::Direct { .. } => {
                Err("sdk auth status requires system-key authentication".to_string())
            }
            AuthMode::SystemKey { .. } => {
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", tenant_id);
                let suffix = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{query}")
                };
                self.send_json(
                    Method::GET,
                    &format!("/sdk/auth/status{suffix}"),
                    Option::<&()>::None,
                )
                .await
            }
        }
    }
}
