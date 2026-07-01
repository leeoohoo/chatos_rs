// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_mcp_runtime::{McpHttpServer, McpStdioServer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMcpConfigRecord {
    pub id: String,
    pub name: String,
    pub transport: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ExternalMcpConfigRecord {
    pub fn to_stdio_server(&self) -> Option<McpStdioServer> {
        if self.transport != "stdio" || !self.enabled {
            return None;
        }
        let command = self.command.as_deref()?.trim();
        if command.is_empty() {
            return None;
        }
        let mut server = McpStdioServer::new(self.name.clone(), command.to_string());
        if !self.args.is_empty() {
            server = server.with_args(self.args.clone());
        }
        if let Some(cwd) = self
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            server = server.with_cwd(cwd.to_string());
        }
        if !self.env.is_empty() {
            server = server.with_env(self.env.clone().into_iter().collect());
        }
        Some(server)
    }

    pub fn to_http_server(&self) -> Option<McpHttpServer> {
        if self.transport != "http" || !self.enabled {
            return None;
        }
        let url = self.url.as_deref()?.trim();
        if url.is_empty() {
            return None;
        }
        let mut server = McpHttpServer::new(self.name.clone(), url.to_string());
        if !self.headers.is_empty() {
            server = server.with_headers(self.headers.clone().into_iter().collect());
        }
        Some(server)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExternalMcpConfigRequest {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateExternalMcpConfigRequest {
    pub name: Option<String>,
    pub transport: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub headers: Option<BTreeMap<String, String>>,
    pub env: Option<BTreeMap<String, String>>,
    pub cwd: Option<String>,
    pub enabled: Option<bool>,
}
