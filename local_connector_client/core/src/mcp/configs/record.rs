// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use crate::config::normalize_optional;
use crate::local_now_rfc3339;
use crate::mcp::manifest::{
    merge_masked_map, LocalMcpConfigDraft, LocalMcpHttpConfig, LocalMcpManifestRecord,
    LocalMcpStdioConfig, LocalMcpTransport,
};

use super::transport::{test_manifest_record, validate_loopback_http_url};

pub(super) fn build_manifest_record(
    owner_user_id: String,
    device_id: String,
    existing: Option<&LocalMcpManifestRecord>,
    manifest_id: &str,
    draft: LocalMcpConfigDraft,
) -> Result<LocalMcpManifestRecord> {
    let display_name = required_text(draft.display_name.clone(), "display_name")?;
    if display_name.chars().count() > 120 {
        return Err(anyhow!("display_name exceeds 120 characters"));
    }
    if existing.is_some_and(|record| {
        record.owner_user_id != owner_user_id || record.device_id != device_id
    }) {
        return Err(anyhow!(
            "MCP manifest does not belong to current user and device"
        ));
    }
    let now = local_now_rfc3339();
    let internal_name = existing
        .map(|record| record.internal_name.clone())
        .unwrap_or_else(|| {
            format!(
                "user_mcp_{}",
                manifest_id
                    .replace('-', "")
                    .chars()
                    .take(12)
                    .collect::<String>()
            )
        });
    let description = normalize_optional(draft.description.as_deref());
    let enabled = draft
        .enabled
        .or_else(|| existing.map(|record| record.enabled))
        .unwrap_or(true);
    let (stdio, http) = configs_for_transport(&draft, existing)?;
    let mut record = LocalMcpManifestRecord {
        manifest_id: manifest_id.to_string(),
        plugin_mcp_id: existing.and_then(|record| record.plugin_mcp_id.clone()),
        owner_user_id,
        device_id,
        internal_name,
        display_name,
        description,
        transport: draft.transport,
        stdio,
        http,
        enabled,
        sync_status: "pending".to_string(),
        last_check_status: "unknown".to_string(),
        last_checked_at: None,
        last_error: None,
        tool_snapshot: Vec::new(),
        manifest_hash: String::new(),
        created_at: existing
            .map(|record| record.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    record.refresh_hash()?;
    Ok(record)
}

fn configs_for_transport(
    draft: &LocalMcpConfigDraft,
    existing: Option<&LocalMcpManifestRecord>,
) -> Result<(Option<LocalMcpStdioConfig>, Option<LocalMcpHttpConfig>)> {
    match draft.transport {
        LocalMcpTransport::Stdio => {
            let command = required_text(draft.command.clone().unwrap_or_default(), "command")?;
            if command.chars().count() > 1024 {
                return Err(anyhow!("command exceeds 1024 characters"));
            }
            let args = draft
                .args
                .iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .take(200)
                .collect::<Vec<_>>();
            let existing_env = existing
                .and_then(|record| record.stdio.as_ref())
                .map(|config| &config.env);
            let env = merge_masked_map(draft.env.clone(), existing_env);
            if env.len() > 200 {
                return Err(anyhow!("env exceeds 200 entries"));
            }
            Ok((Some(LocalMcpStdioConfig { command, args, env }), None))
        }
        LocalMcpTransport::Http => {
            let url = required_text(draft.url.clone().unwrap_or_default(), "url")?;
            validate_loopback_http_url(url.as_str())?;
            let existing_headers = existing
                .and_then(|record| record.http.as_ref())
                .map(|config| &config.headers);
            let headers = merge_masked_map(draft.headers.clone(), existing_headers);
            if headers.len() > 100 {
                return Err(anyhow!("headers exceed 100 entries"));
            }
            Ok((
                None,
                Some(LocalMcpHttpConfig {
                    url,
                    headers,
                    timeout_ms: draft.timeout_ms.unwrap_or(15_000).clamp(300, 120_000),
                }),
            ))
        }
    }
}

pub(super) async fn apply_test_result(record: &mut LocalMcpManifestRecord) {
    let result = test_manifest_record(record).await;
    apply_manifest_test_result(record, result);
}

pub(super) fn apply_manifest_test_result(
    record: &mut LocalMcpManifestRecord,
    result: Result<Vec<serde_json::Value>>,
) {
    match result {
        Ok(tools) => {
            record.last_check_status = "available".to_string();
            record.last_checked_at = Some(local_now_rfc3339());
            record.last_error = None;
            record.tool_snapshot = tools;
        }
        Err(error) => {
            record.last_check_status = "invalid".to_string();
            record.last_checked_at = Some(local_now_rfc3339());
            record.last_error = Some(sanitize_manifest_error(record, error.to_string().as_str()));
            record.tool_snapshot.clear();
        }
    }
}

pub(super) fn sanitize_manifest_error(record: &LocalMcpManifestRecord, error: &str) -> String {
    let mut out = error.to_string();
    for secret in record
        .stdio
        .iter()
        .flat_map(|config| config.env.values())
        .chain(
            record
                .http
                .iter()
                .flat_map(|config| config.headers.values()),
        )
    {
        if !secret.is_empty() {
            out = out.replace(secret, "[REDACTED]");
        }
    }
    out.chars().take(1000).collect()
}

fn required_text(value: String, field: &str) -> Result<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(anyhow!("{field} is required"))
    } else {
        Ok(value)
    }
}
