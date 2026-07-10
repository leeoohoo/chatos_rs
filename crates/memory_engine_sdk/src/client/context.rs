// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use crate::models::{ComposeContextRequest, ComposeContextResponse, SdkComposeContextRequest};

use super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn compose_context(
        &self,
        req: &SdkComposeContextRequest,
    ) -> Result<ComposeContextResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "compose_context")?;
                let direct = ComposeContextRequest {
                    tenant_id: req.tenant_id.clone(),
                    source_id: source_id.to_string(),
                    subject_id: req.subject_id.clone(),
                    related_subject_ids: req.related_subject_ids.clone(),
                    thread_id: req.thread_id.clone(),
                    policy: req.policy.clone(),
                };
                self.send_json(Method::POST, "/context/compose", Some(&direct))
                    .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(Method::POST, "/sdk/context/compose", Some(req))
                    .await
            }
        }
    }
}
