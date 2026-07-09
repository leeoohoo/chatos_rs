// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::task::JoinSet;
use tracing::warn;

use crate::naming::{canonical_prefixed_tool_name, legacy_prefixed_tool_name};
use crate::rpc::{list_tools_http, list_tools_stdio};
use crate::schema::{build_function_tool_schema, parse_tool_definition};
use crate::types::{McpStdioServer, ParsedToolDefinition, ToolInfo};

use super::McpExecutor;

impl McpExecutor {
    fn register_available_tool(
        &mut self,
        server_name: &str,
        server_type: &str,
        server_url: Option<String>,
        server_headers: Option<HashMap<String, String>>,
        server_timeout: Option<Duration>,
        server_config: Option<McpStdioServer>,
        def: ParsedToolDefinition,
        tool: Value,
    ) {
        let public_name = self.reserve_tool_name(server_name, def.name.as_str());
        self.register_tool_aliases(server_name, def.name.as_str(), public_name.as_str());
        self.available_tools.push(build_function_tool_schema(
            public_name.as_str(),
            def.description.as_str(),
            &def.parameters,
        ));
        self.tool_metadata.insert(
            public_name,
            ToolInfo {
                original_name: def.name,
                server_name: server_name.to_string(),
                server_type: server_type.to_string(),
                server_url,
                server_headers,
                server_timeout,
                server_config,
                tool_info: tool,
            },
        );
    }
    fn reserve_tool_name(&self, server_name: &str, tool_name: &str) -> String {
        let canonical = canonical_prefixed_tool_name(server_name, tool_name);
        if !self.tool_metadata.contains_key(canonical.as_str()) {
            return canonical;
        }

        let hashed = format!(
            "{}_{:08x}",
            canonical,
            stable_tool_name_hash(server_name, tool_name)
        );
        if !self.tool_metadata.contains_key(hashed.as_str()) {
            return hashed;
        }

        let mut counter = 2usize;
        loop {
            let candidate = format!("{hashed}_{counter}");
            if !self.tool_metadata.contains_key(candidate.as_str()) {
                return candidate;
            }
            counter += 1;
        }
    }
    fn register_tool_aliases(&mut self, server_name: &str, tool_name: &str, public_name: &str) {
        let legacy = legacy_prefixed_tool_name(server_name, tool_name);
        if legacy != public_name {
            self.tool_aliases
                .entry(legacy)
                .or_insert_with(|| public_name.to_string());
        }
    }
    pub(in crate::executor) async fn register_http_tools(&mut self) {
        let server_count = self.http_servers.len();
        let mut ordered = (0..server_count).map(|_| None).collect::<Vec<_>>();
        let mut joins = JoinSet::new();
        for (index, server) in self.http_servers.clone().into_iter().enumerate() {
            joins.spawn(async move {
                let tools = list_tools_http(
                    server.url.as_str(),
                    server.headers.as_ref(),
                    server.timeout_duration(),
                )
                .await;
                (index, server, tools)
            });
        }

        while let Some(joined) = joins.join_next().await {
            match joined {
                Ok((index, server, tools)) => ordered[index] = Some((server, tools)),
                Err(err) => {
                    warn!("[MCP] HTTP tools/list task failed: {err}");
                }
            }
        }

        for (server, tools) in ordered.into_iter().flatten() {
            match tools {
                Ok(tools) => {
                    for tool in tools {
                        if let Some(def) = parse_tool_definition(&tool) {
                            self.register_available_tool(
                                server.name.as_str(),
                                "http",
                                Some(server.url.clone()),
                                server.headers.clone(),
                                server.timeout_duration(),
                                None,
                                def,
                                tool,
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        server_name = server.name.as_str(),
                        server_url = server.url.as_str(),
                        error = err.as_str(),
                        "failed to register HTTP MCP tools"
                    );
                    self.unavailable_tools.push(unavailable_server(
                        server.name.as_str(),
                        "http",
                        err.as_str(),
                    ));
                }
            }
        }
    }
    pub(in crate::executor) async fn register_stdio_tools(&mut self) {
        let server_count = self.stdio_servers.len();
        let mut ordered = (0..server_count).map(|_| None).collect::<Vec<_>>();
        let mut joins = JoinSet::new();
        for (index, server) in self.stdio_servers.clone().into_iter().enumerate() {
            joins.spawn(async move {
                let tools = list_tools_stdio(&server).await;
                (index, server, tools)
            });
        }

        while let Some(joined) = joins.join_next().await {
            match joined {
                Ok((index, server, tools)) => ordered[index] = Some((server, tools)),
                Err(err) => {
                    warn!("[MCP] stdio tools/list task failed: {err}");
                }
            }
        }

        for (server, tools) in ordered.into_iter().flatten() {
            match tools {
                Ok(tools) => {
                    for tool in tools {
                        if let Some(def) = parse_tool_definition(&tool) {
                            self.register_available_tool(
                                server.name.as_str(),
                                "stdio",
                                None,
                                None,
                                None,
                                Some(server.clone()),
                                def,
                                tool,
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        server_name = server.name.as_str(),
                        command = server.command.as_str(),
                        error = err.as_str(),
                        "failed to register stdio MCP tools"
                    );
                    self.unavailable_tools.push(unavailable_server(
                        server.name.as_str(),
                        "stdio",
                        err.as_str(),
                    ));
                }
            }
        }
    }
    pub(in crate::executor) fn register_builtin_tools(&mut self) {
        for server in self.builtin_servers.clone() {
            let Some(provider) = self.builtin_registry.get(server.name.as_str()) else {
                self.unavailable_tools.push(unavailable_server(
                    server.name.as_str(),
                    "builtin",
                    "missing builtin provider",
                ));
                continue;
            };
            for (tool_name, reason) in provider.unavailable_tools() {
                self.unavailable_tools.push(json!({
                    "server_name": server.name,
                    "server_type": "builtin",
                    "tool_name": tool_name,
                    "reason": reason
                }));
            }
            for tool in provider.list_tools() {
                if let Some(def) = parse_tool_definition(&tool) {
                    self.register_available_tool(
                        server.name.as_str(),
                        "builtin",
                        None,
                        None,
                        None,
                        None,
                        def,
                        tool,
                    );
                }
            }
        }
    }
}

fn stable_tool_name_hash(server_name: &str, tool_name: &str) -> u32 {
    const OFFSET: u32 = 0x811c9dc5;
    const PRIME: u32 = 0x01000193;

    let mut hash = OFFSET;
    for byte in server_name
        .as_bytes()
        .iter()
        .copied()
        .chain(std::iter::once(0xff))
        .chain(tool_name.as_bytes().iter().copied())
    {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
fn unavailable_server(server_name: &str, server_type: &str, reason: &str) -> Value {
    json!({
        "server_name": server_name,
        "server_type": server_type,
        "reason": reason
    })
}
