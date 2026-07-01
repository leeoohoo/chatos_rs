// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use crate::models::{
    EngineSource, ListResponse, ListSourcesRequest, RotateSourceSecretResponse, UpsertSourceRequest,
};

use super::super::transport::{
    append_optional_bool_query, append_optional_i64_query, append_optional_query,
};
use super::super::MemoryEngineClient;

fn build_list_sources_suffix(req: &ListSourcesRequest) -> String {
    let mut query = String::new();
    append_optional_query(&mut query, "tenant_id", req.tenant_id.as_deref());
    append_optional_query(&mut query, "source_type", req.source_type.as_deref());
    append_optional_query(&mut query, "status", req.status.as_deref());
    append_optional_bool_query(&mut query, "sdk_enabled", req.sdk_enabled);
    append_optional_i64_query(&mut query, "limit", req.limit);
    append_optional_i64_query(&mut query, "offset", req.offset);
    if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    }
}

fn build_rotate_source_secret_path(source_id: &str, tenant_id: Option<&str>) -> String {
    let mut query = String::new();
    append_optional_query(&mut query, "tenant_id", tenant_id);
    format!(
        "/admin/sources/{}/rotate-key{}",
        urlencoding::encode(source_id),
        if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        }
    )
}

impl MemoryEngineClient {
    pub async fn list_sources(
        &self,
        req: &ListSourcesRequest,
    ) -> Result<Vec<EngineSource>, String> {
        let suffix = build_list_sources_suffix(req);
        let resp: ListResponse<EngineSource> = self
            .send_json(
                Method::GET,
                &format!("/admin/sources{suffix}"),
                Option::<&()>::None,
            )
            .await?;
        Ok(resp.items)
    }

    pub async fn upsert_source(
        &self,
        source_id: &str,
        req: &UpsertSourceRequest,
    ) -> Result<EngineSource, String> {
        self.send_json(
            Method::PUT,
            &format!("/admin/sources/{}", urlencoding::encode(source_id)),
            Some(req),
        )
        .await
    }

    pub async fn rotate_source_secret(
        &self,
        source_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<RotateSourceSecretResponse, String> {
        self.send_json(
            Method::POST,
            &build_rotate_source_secret_path(source_id, tenant_id),
            Option::<&()>::None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{build_list_sources_suffix, build_rotate_source_secret_path};
    use crate::models::ListSourcesRequest;

    #[test]
    fn build_list_sources_suffix_encodes_optional_filters() {
        let suffix = build_list_sources_suffix(&ListSourcesRequest {
            tenant_id: Some("tenant-1".to_string()),
            source_type: Some("sdk managed".to_string()),
            status: Some("active".to_string()),
            sdk_enabled: Some(true),
            limit: Some(50),
            offset: Some(10),
        });

        assert_eq!(
            suffix,
            "?tenant_id=tenant-1&source_type=sdk%20managed&status=active&sdk_enabled=true&limit=50&offset=10"
        );
    }

    #[test]
    fn build_rotate_source_secret_path_omits_or_includes_tenant_query() {
        assert_eq!(
            build_rotate_source_secret_path("source/1", None),
            "/admin/sources/source%2F1/rotate-key"
        );
        assert_eq!(
            build_rotate_source_secret_path("source/1", Some("tenant a")),
            "/admin/sources/source%2F1/rotate-key?tenant_id=tenant%20a"
        );
    }
}
