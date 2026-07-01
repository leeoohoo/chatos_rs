// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use super::super::transport::append_optional_query;
use super::super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn delete_record(
        &self,
        record_id: &str,
        tenant_id: Option<&str>,
        thread_id: Option<&str>,
    ) -> Result<bool, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "delete_record")?;
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", tenant_id);
                append_optional_query(&mut query, "source_id", Some(source_id));
                append_optional_query(&mut query, "thread_id", thread_id);
                let full_path = if query.is_empty() {
                    format!("/records/{}", urlencoding::encode(record_id))
                } else {
                    format!("/records/{}?{query}", urlencoding::encode(record_id))
                };
                #[derive(serde::Deserialize)]
                struct DeleteResponse {
                    deleted: bool,
                }
                let resp: DeleteResponse = self
                    .send_json(Method::DELETE, full_path.as_str(), Option::<&()>::None)
                    .await?;
                Ok(resp.deleted)
            }
            AuthMode::SystemKey { .. } => {
                let Some(tenant_id) = tenant_id else {
                    return Err("tenant_id is required for system-key record deletion".to_string());
                };
                let mut query_pairs = vec![("tenant_id", tenant_id)];
                if let Some(thread_id) = thread_id.map(str::trim).filter(|value| !value.is_empty())
                {
                    query_pairs.push(("thread_id", thread_id));
                }
                self.delete_with_query(
                    &format!("/sdk/records/{}", urlencoding::encode(record_id)),
                    query_pairs.as_slice(),
                )
                .await
            }
        }
    }
}
