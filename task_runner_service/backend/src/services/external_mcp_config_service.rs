// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_mcp_runtime::{list_tools_http, list_tools_stdio, parse_tool_definition};
use serde_json::Value;
use tracing::info;

use super::*;

impl ExternalMcpConfigService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }

    async fn first_task_referencing_config(
        &self,
        config_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .find(|task| {
                task.mcp_config
                    .external_mcp_config_ids
                    .iter()
                    .any(|id| id == config_id)
            })
            .map(|task| task.id))
    }

    pub async fn list_external_mcp_configs(&self) -> Result<Vec<ExternalMcpConfigRecord>, String> {
        self.store.list_external_mcp_configs().await
    }

    pub async fn get_external_mcp_config(
        &self,
        id: &str,
    ) -> Result<Option<ExternalMcpConfigRecord>, String> {
        self.store.get_external_mcp_config(id).await
    }

    pub async fn create_external_mcp_config(
        &self,
        input: CreateExternalMcpConfigRequest,
        creator: Option<&CurrentUser>,
    ) -> Result<ExternalMcpConfigRecord, String> {
        let now = now_rfc3339();
        let record = ExternalMcpConfigRecord {
            id: Uuid::new_v4().to_string(),
            name: normalize_required("name", input.name)?,
            transport: normalize_transport(&input.transport)?,
            command: normalized_optional(input.command),
            args: normalize_string_list(input.args),
            url: normalized_optional(input.url),
            headers: normalize_string_map(input.headers),
            env: normalize_string_map(input.env),
            cwd: normalized_optional(input.cwd),
            enabled: input.enabled.unwrap_or(true),
            creator_user_id: creator.map(|user| user.id.clone()),
            creator_username: creator.map(|user| user.username.clone()),
            creator_display_name: creator.map(|user| user.display_name.clone()),
            owner_user_id: creator
                .and_then(|user| user.effective_owner_user_id().map(ToOwned::to_owned)),
            owner_username: creator
                .and_then(|user| user.effective_owner_username().map(ToOwned::to_owned)),
            owner_display_name: creator.and_then(|user| {
                user.effective_owner_display_name()
                    .map(ToOwned::to_owned)
                    .or_else(|| user.effective_owner_username().map(ToOwned::to_owned))
            }),
            created_at: now.clone(),
            updated_at: now,
        };
        validate_external_mcp_config(&record)?;
        test_external_mcp_config(&record).await?;
        self.store.save_external_mcp_config(record).await
    }

    pub async fn update_external_mcp_config(
        &self,
        id: &str,
        patch: UpdateExternalMcpConfigRequest,
    ) -> Result<Option<ExternalMcpConfigRecord>, String> {
        let Some(mut record) = self.store.get_external_mcp_config(id).await? else {
            return Ok(None);
        };

        if let Some(name) = patch.name {
            record.name = normalize_required("name", name)?;
        }
        if let Some(transport) = patch.transport {
            record.transport = normalize_transport(&transport)?;
        }
        if patch.command.is_some() {
            record.command = normalized_optional(patch.command);
        }
        if let Some(args) = patch.args {
            record.args = normalize_string_list(args);
        }
        if patch.url.is_some() {
            record.url = normalized_optional(patch.url);
        }
        if let Some(headers) = patch.headers {
            record.headers = normalize_string_map(headers);
        }
        if let Some(env) = patch.env {
            record.env = normalize_string_map(env);
        }
        if patch.cwd.is_some() {
            record.cwd = normalized_optional(patch.cwd);
        }
        if let Some(enabled) = patch.enabled {
            if !enabled {
                if let Some(task_id) = self.first_task_referencing_config(id).await? {
                    return Err(format!(
                        "外部 MCP 配置仍被任务引用，暂时不能停用: {task_id}"
                    ));
                }
            }
            record.enabled = enabled;
        }

        validate_external_mcp_config(&record)?;
        if record.enabled {
            test_external_mcp_config(&record).await?;
        }
        record.updated_at = now_rfc3339();
        Ok(Some(self.store.save_external_mcp_config(record).await?))
    }

    pub async fn delete_external_mcp_config(&self, id: &str) -> Result<bool, String> {
        if let Some(task_id) = self.first_task_referencing_config(id).await? {
            return Err(format!(
                "外部 MCP 配置仍被任务引用，暂时不能删除: {task_id}"
            ));
        }
        self.store.delete_external_mcp_config(id).await
    }
}

fn normalize_required(label: &str, value: String) -> Result<String, String> {
    validate_required(label, &value)?;
    Ok(value.trim().to_string())
}

fn normalize_transport(value: &str) -> Result<String, String> {
    match value.trim() {
        "stdio" => Ok("stdio".to_string()),
        "http" => Ok("http".to_string()),
        _ => Err("transport 仅支持 stdio / http".to_string()),
    }
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_string_map(values: BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_string();
            if key.is_empty() {
                None
            } else {
                Some((key, value.trim().to_string()))
            }
        })
        .collect()
}

fn validate_external_mcp_config(record: &ExternalMcpConfigRecord) -> Result<(), String> {
    validate_required("name", &record.name)?;
    match record.transport.as_str() {
        "stdio" => {
            if record
                .command
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err("stdio 类型需要提供 command".to_string());
            }
        }
        "http" => {
            let Some(url) = record
                .url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return Err("http 类型需要提供 url".to_string());
            };
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err("http 类型 url 必须以 http:// 或 https:// 开头".to_string());
            }
        }
        _ => return Err("transport 仅支持 stdio / http".to_string()),
    }
    Ok(())
}

async fn test_external_mcp_config(record: &ExternalMcpConfigRecord) -> Result<(), String> {
    let tools = match record.transport.as_str() {
        "http" => {
            let server = record
                .to_http_server()
                .ok_or_else(|| "外部 MCP 配置无效: http 类型需要可用 url".to_string())?;
            list_tools_http(server.url.as_str(), server.headers.as_ref())
                .await
                .map_err(|err| {
                    format!(
                        "外部 MCP 连通性测试失败: {} ({}) tools/list 调用失败: {err}",
                        record.name, record.transport
                    )
                })?
        }
        "stdio" => {
            let server = record
                .to_stdio_server()
                .ok_or_else(|| "外部 MCP 配置无效: stdio 类型需要可用 command".to_string())?;
            list_tools_stdio(&server).await.map_err(|err| {
                format!(
                    "外部 MCP 连通性测试失败: {} ({}) tools/list 调用失败: {err}",
                    record.name, record.transport
                )
            })?
        }
        _ => return Err("transport 仅支持 stdio / http".to_string()),
    };
    let tool_names = valid_mcp_tool_names(&tools);
    if tool_names.is_empty() {
        return Err(format!(
            "外部 MCP 连通性测试失败: {} ({}) tools/list 未返回可识别工具",
            record.name, record.transport
        ));
    }
    info!(
        external_mcp_config_id = record.id.as_str(),
        external_mcp_config_name = record.name.as_str(),
        external_mcp_transport = record.transport.as_str(),
        external_mcp_tool_count = tool_names.len(),
        external_mcp_tools = %tool_names.join(","),
        "external MCP config connectivity test passed"
    );
    Ok(())
}

fn valid_mcp_tool_names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .filter_map(parse_tool_definition)
        .map(|definition| definition.name)
        .collect()
}
