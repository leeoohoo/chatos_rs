// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use chatos_builtin_tools::{WebToolsOptions, WebToolsService};
use serde_json::{json, Value};
use url::Url;

use crate::mcp::tools::{local_browser_conversation_id, local_browser_tools_service_for_root};
use crate::relay::RelayRequest;
use crate::workspace::paths::{canonicalize_existing_dir, workspace_for_request};
use crate::LocalState;

use super::required_text;

pub(super) fn tool_definitions(
    skill_id: &str,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Vec<Value>> {
    match skill_id {
        "internal_skill_openai_docs" => {
            Ok(vec![search_openai_docs_tool(), extract_openai_docs_tool()])
        }
        "internal_skill_browser" => {
            if let Some(error) = browser_dependency_error() {
                return Err(anyhow!(error));
            }
            let service = browser_service(state, request)?;
            let tools = service.list_tools();
            if tools.is_empty() {
                Err(anyhow!("local browser adapter did not publish any tools"))
            } else {
                Ok(tools)
            }
        }
        _ => Ok(Vec::new()),
    }
}

pub(super) fn dependency_error(skill_id: &str) -> Option<String> {
    (skill_id == "internal_skill_browser")
        .then(browser_dependency_error)
        .flatten()
}

pub(super) fn execute(
    skill_id: &str,
    operation: &str,
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Option<Result<Value>> {
    let result = match (skill_id, operation) {
        ("internal_skill_openai_docs", "search_openai_docs") => {
            search_openai_docs(arguments, state, request)
        }
        ("internal_skill_openai_docs", "extract_openai_docs") => {
            extract_openai_docs(arguments, state, request)
        }
        ("internal_skill_browser", operation) => {
            execute_browser_tool(operation, arguments, state, request)
        }
        _ => return None,
    };
    Some(result)
}

fn execute_browser_tool(
    operation: &str,
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    if let Some(error) = browser_dependency_error() {
        return Err(anyhow!(error));
    }
    let service = browser_service(state, request)?;
    let conversation_id = local_browser_conversation_id(request);
    service
        .call_tool(operation, arguments.clone(), Some(conversation_id.as_str()))
        .map_err(|err| anyhow!(err))
}

fn browser_service(
    state: &LocalState,
    request: &RelayRequest,
) -> Result<chatos_builtin_tools::BrowserToolsService> {
    if request.workspace_id.trim().is_empty() {
        return Err(anyhow!(
            "workspace_id is required for local browser control"
        ));
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    local_browser_tools_service_for_root(workspace.absolute_root.as_path(), request)
}

fn browser_dependency_error() -> Option<String> {
    if resolve_agent_browser_binary().is_some() {
        None
    } else {
        Some(
            "agent-browser executable is not installed in the Local Connector runtime; browser Skill remains unavailable"
                .to_string(),
        )
    }
}

fn resolve_agent_browser_binary() -> Option<PathBuf> {
    if let Some(path) = env::var_os("AGENT_BROWSER_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join("agent-browser");
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = directory.join("agent-browser.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn search_openai_docs_tool() -> Value {
    json!({
        "name":"search_openai_docs",
        "description":"Search current OpenAI official documentation from the Local Connector device and return source URLs.",
        "inputSchema":{
            "type":"object",
            "properties":{
                "query":{"type":"string"},
                "limit":{"type":"integer","minimum":1,"maximum":10,"default":5}
            },
            "required":["query"],
            "additionalProperties":false
        }
    })
}

fn extract_openai_docs_tool() -> Value {
    json!({
        "name":"extract_openai_docs",
        "description":"Extract text from up to five HTTPS URLs on official OpenAI domains. Non-OpenAI URLs are rejected locally.",
        "inputSchema":{
            "type":"object",
            "properties":{
                "urls":{
                    "type":"array",
                    "items":{"type":"string"},
                    "minItems":1,
                    "maxItems":5
                }
            },
            "required":["urls"],
            "additionalProperties":false
        }
    })
}

fn search_openai_docs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let query = required_text(arguments, "query")?;
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .clamp(1, 10);
    let service = openai_docs_web_service(state, request)?;
    service
        .call_tool(
            "web_search",
            json!({
                "query": format!(
                    "site:developers.openai.com OR site:platform.openai.com OR site:help.openai.com OR site:openai.com {query}"
                ),
                "limit": limit,
            }),
        )
        .map_err(|err| anyhow!(err))
}

fn extract_openai_docs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let urls = arguments
        .get("urls")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("urls must be an array"))?;
    if urls.is_empty() || urls.len() > 5 {
        return Err(anyhow!("urls must contain between 1 and 5 items"));
    }
    let urls = urls
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("urls contains an invalid URL"))?;
            validate_openai_docs_url(value)?;
            Ok(value.to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let service = openai_docs_web_service(state, request)?;
    service
        .call_tool("web_extract", json!({"urls": urls}))
        .map_err(|err| anyhow!(err))
}

fn openai_docs_web_service(state: &LocalState, request: &RelayRequest) -> Result<WebToolsService> {
    if request.workspace_id.trim().is_empty() {
        return Err(anyhow!("workspace_id is required for OpenAI Docs"));
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    WebToolsService::new(WebToolsOptions {
        server_name: "local_skill_openai_docs".to_string(),
        workspace_dir: root,
        request_timeout_seconds: 30,
        default_search_limit: 5,
        max_search_limit: 10,
        max_extract_urls: 5,
        max_extract_chars: 100_000,
    })
    .map_err(|err| anyhow!(err))
}

fn validate_openai_docs_url(value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|err| anyhow!("invalid OpenAI docs URL: {err}"))?;
    if url.scheme() != "https" {
        return Err(anyhow!("OpenAI docs URL must use HTTPS"));
    }
    let host = url
        .host_str()
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("OpenAI docs URL is missing a host"))?;
    if host != "openai.com" && !host.ends_with(".openai.com") {
        return Err(anyhow!(
            "URL is outside the allowed OpenAI official domains"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{dependency_error, validate_openai_docs_url};

    #[test]
    fn openai_docs_extraction_rejects_non_official_domains() {
        assert!(validate_openai_docs_url("https://developers.openai.com/api/docs").is_ok());
        assert!(validate_openai_docs_url("https://help.openai.com/en/articles/123").is_ok());
        assert!(validate_openai_docs_url("https://openai.com.evil.example/docs").is_err());
        assert!(validate_openai_docs_url("http://platform.openai.com/docs").is_err());
    }

    #[test]
    fn browser_dependency_check_never_accepts_npx_as_the_runtime() {
        let error = dependency_error("internal_skill_browser");
        if error.is_none() {
            assert!(
                std::env::var_os("AGENT_BROWSER_BIN").is_some()
                    || std::env::var_os("PATH").is_some()
            );
        }
    }
}
