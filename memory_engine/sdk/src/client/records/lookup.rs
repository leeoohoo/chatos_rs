use reqwest::Method;

use crate::models::{EngineRecord, SdkGetRecordRequest};

use super::super::transport::append_optional_query;
use super::super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn get_record(
        &self,
        record_id: &str,
        tenant_id: Option<&str>,
        thread_id: Option<&str>,
    ) -> Result<Option<EngineRecord>, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "get_record")?;
                #[derive(serde::Deserialize)]
                struct GetRecordResponse {
                    item: Option<EngineRecord>,
                }
                let mut query = String::new();
                append_optional_query(&mut query, "tenant_id", tenant_id);
                append_optional_query(&mut query, "source_id", Some(source_id));
                append_optional_query(&mut query, "thread_id", thread_id);
                let full_path = if query.is_empty() {
                    format!("/records/{}", urlencoding::encode(record_id))
                } else {
                    format!("/records/{}?{query}", urlencoding::encode(record_id))
                };
                let resp: GetRecordResponse = self
                    .send_json(Method::GET, full_path.as_str(), Option::<&()>::None)
                    .await?;
                Ok(resp.item)
            }
            AuthMode::SystemKey { .. } => {
                let Some(tenant_id) = tenant_id else {
                    return Err("tenant_id is required for system-key record lookup".to_string());
                };
                #[derive(serde::Deserialize)]
                struct GetRecordResponse {
                    item: Option<EngineRecord>,
                }
                let resp: GetRecordResponse = self
                    .send_json(
                        Method::POST,
                        &format!("/sdk/records/{}", urlencoding::encode(record_id)),
                        Some(&SdkGetRecordRequest {
                            tenant_id: tenant_id.to_string(),
                            thread_id: thread_id.map(ToOwned::to_owned),
                        }),
                    )
                    .await?;
                Ok(resp.item)
            }
        }
    }
}
