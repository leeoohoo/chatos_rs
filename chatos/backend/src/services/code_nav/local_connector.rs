// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::path::Path;

use super::types::{
    DocumentSymbolItem, DocumentSymbolsRequest, DocumentSymbolsResponse, NavCapabilitiesResponse,
    NavLocation, NavLocationsResponse, NavPositionRequest,
};
use super::workspace::detect_language;
use crate::api::local_connectors::{
    call_local_mcp_tool, local_connector_root_path, parse_local_connector_root_path,
    LocalConnectorRootRef, LOCAL_CONNECTOR_BUILTIN_CODE_READ,
};

#[derive(Debug, Clone)]
struct LocalCodeNavContext {
    root_ref: LocalConnectorRootRef,
    root_relative: String,
    file_relative: String,
    file_project_relative: String,
    language: String,
}

pub fn is_local_connector_request(project_root: &str) -> bool {
    parse_local_connector_root_path(project_root).is_some()
}

pub async fn capabilities(
    project_root: &str,
    file_path: &str,
) -> Result<NavCapabilitiesResponse, String> {
    let ctx = parse_context(project_root, file_path)?;
    Ok(NavCapabilitiesResponse {
        language: ctx.language,
        provider: "local_connector_fallback".to_string(),
        supports_definition: true,
        supports_references: true,
        supports_document_symbols: true,
        fallback_available: true,
    })
}

pub async fn definition(request: &NavPositionRequest) -> Result<NavLocationsResponse, String> {
    let ctx = parse_context(&request.project_root, &request.file_path)?;
    let content = read_local_file(&ctx).await?;
    let token = extract_token_at_position(content.as_str(), request.line, request.column);
    let Some(token) = token else {
        return Ok(nav_response(&ctx, "heuristic", None, Vec::new()));
    };
    let hits = search_local_project(&ctx, token.as_str(), 240).await?;
    let mut locations = hits
        .into_iter()
        .filter_map(|hit| {
            let score = rank_definition_candidate(&ctx, request, token.as_str(), &hit);
            (score >= 2.0).then(|| hit.into_location(&ctx, token.as_str(), score))
        })
        .filter(|location| !is_request_location(&ctx, request, location))
        .collect::<Vec<_>>();
    locations.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
    });
    locations.truncate(20);
    Ok(nav_response(&ctx, "heuristic", Some(token), locations))
}

pub async fn references(request: &NavPositionRequest) -> Result<NavLocationsResponse, String> {
    let ctx = parse_context(&request.project_root, &request.file_path)?;
    let content = read_local_file(&ctx).await?;
    let token = extract_token_at_position(content.as_str(), request.line, request.column);
    let Some(token) = token else {
        return Ok(nav_response(&ctx, "text-search", None, Vec::new()));
    };
    let mut locations = search_local_project(&ctx, token.as_str(), 300)
        .await?
        .into_iter()
        .filter(|hit| line_contains_word(hit.text.as_str(), token.as_str()))
        .map(|hit| {
            let score = if hit.relative_path == ctx.file_project_relative {
                1.5
            } else {
                1.0
            };
            hit.into_location(&ctx, token.as_str(), score)
        })
        .filter(|location| !is_request_location(&ctx, request, location))
        .collect::<Vec<_>>();
    locations.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    locations.truncate(100);
    Ok(nav_response(&ctx, "text-search", Some(token), locations))
}

pub async fn document_symbols(
    request: &DocumentSymbolsRequest,
) -> Result<DocumentSymbolsResponse, String> {
    let ctx = parse_context(&request.project_root, &request.file_path)?;
    let content = read_local_file(&ctx).await?;
    let symbols = extract_document_symbols(content.as_str());
    Ok(DocumentSymbolsResponse {
        provider: "local_connector_fallback".to_string(),
        language: ctx.language,
        mode: "heuristic".to_string(),
        symbols,
    })
}

fn parse_context(project_root: &str, file_path: &str) -> Result<LocalCodeNavContext, String> {
    let root_ref = parse_local_connector_root_path(project_root)
        .ok_or_else(|| "Local Connector project_root 格式错误".to_string())?;
    let file_ref = parse_local_connector_root_path(file_path)
        .ok_or_else(|| "Local Connector file_path 格式错误".to_string())?;
    if root_ref.device_id != file_ref.device_id || root_ref.workspace_id != file_ref.workspace_id {
        return Err("file_path 超出项目根目录".to_string());
    }
    let root_relative = root_ref.relative_path.clone().unwrap_or_default();
    let file_relative = file_ref.relative_path.clone().unwrap_or_default();
    let file_project_relative = if root_relative.is_empty() {
        file_relative.clone()
    } else if file_relative == root_relative {
        String::new()
    } else if let Some(relative) = file_relative.strip_prefix(format!("{root_relative}/").as_str())
    {
        relative.to_string()
    } else {
        return Err("file_path 超出项目根目录".to_string());
    };
    if file_project_relative.trim().is_empty() {
        return Err("file_path 不是文件".to_string());
    }
    let language = detect_language(Path::new(file_relative.as_str()));
    Ok(LocalCodeNavContext {
        root_ref,
        root_relative,
        file_relative,
        file_project_relative,
        language,
    })
}

async fn read_local_file(ctx: &LocalCodeNavContext) -> Result<String, String> {
    let value = call_local_mcp_tool(
        ctx.root_ref.device_id.as_str(),
        ctx.root_ref.workspace_id.as_str(),
        None,
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "read_file_raw",
        json!({ "path": ctx.file_relative, "with_line_numbers": false }),
    )
    .await
    .map_err(connector_error_message)?;
    Ok(value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

async fn search_local_project(
    ctx: &LocalCodeNavContext,
    query: &str,
    max_results: usize,
) -> Result<Vec<LocalSearchHit>, String> {
    let value = call_local_mcp_tool(
        ctx.root_ref.device_id.as_str(),
        ctx.root_ref.workspace_id.as_str(),
        None,
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "search_text",
        json!({
            "path": local_search_root(ctx),
            "pattern": query,
            "max_results": max_results,
        }),
    )
    .await
    .map_err(connector_error_message)?;
    let matches = value
        .get("results")
        .or_else(|| value.get("matches"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(matches
        .into_iter()
        .filter_map(|item| {
            let path = item.get("path").and_then(Value::as_str)?.to_string();
            let line = item.get("line").and_then(Value::as_u64).unwrap_or(1) as usize;
            let text = item
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let relative_path = project_relative_path(ctx, path.as_str())?;
            Some(LocalSearchHit {
                path,
                relative_path,
                line,
                text,
            })
        })
        .collect())
}

#[derive(Debug, Clone)]
struct LocalSearchHit {
    path: String,
    relative_path: String,
    line: usize,
    text: String,
}

impl LocalSearchHit {
    fn into_location(self, ctx: &LocalCodeNavContext, token: &str, score: f64) -> NavLocation {
        let column = token_column(self.text.as_str(), token).unwrap_or(1);
        NavLocation {
            path: local_connector_root_path(
                ctx.root_ref.device_id.as_str(),
                ctx.root_ref.workspace_id.as_str(),
                Some(self.path.as_str()),
            ),
            relative_path: self.relative_path,
            line: self.line,
            column,
            end_line: self.line,
            end_column: column + token.chars().count().saturating_sub(1),
            preview: self.text,
            score,
        }
    }
}

fn nav_response(
    ctx: &LocalCodeNavContext,
    mode: &str,
    token: Option<String>,
    locations: Vec<NavLocation>,
) -> NavLocationsResponse {
    NavLocationsResponse {
        provider: "local_connector_fallback".to_string(),
        language: ctx.language.clone(),
        mode: mode.to_string(),
        token,
        locations,
    }
}

fn extract_token_at_position(content: &str, line: usize, column: usize) -> Option<String> {
    let lines = content.lines().collect::<Vec<_>>();
    if line == 0 || line > lines.len() {
        return None;
    }
    let current = lines[line - 1];
    let chars = current.char_indices().collect::<Vec<_>>();
    if chars.is_empty() {
        return None;
    }
    let mut index = column.saturating_sub(1).min(chars.len().saturating_sub(1));
    if !is_token_char(chars[index].1) && index > 0 && is_token_char(chars[index - 1].1) {
        index -= 1;
    }
    if !is_token_char(chars[index].1) {
        return None;
    }
    let mut start = index;
    while start > 0 && is_token_char(chars[start - 1].1) {
        start -= 1;
    }
    let mut end = index;
    while end + 1 < chars.len() && is_token_char(chars[end + 1].1) {
        end += 1;
    }
    let start_byte = chars[start].0;
    let end_byte = if end + 1 < chars.len() {
        chars[end + 1].0
    } else {
        current.len()
    };
    Some(current[start_byte..end_byte].to_string())
}

fn extract_document_symbols(content: &str) -> Vec<DocumentSymbolItem> {
    let mut symbols = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let symbol = symbol_from_line(trimmed);
        if let Some((kind, name)) = symbol {
            if name.is_empty() {
                continue;
            }
            let column = line
                .find(name.as_str())
                .map(|offset| line[..offset].chars().count() + 1)
                .unwrap_or(1);
            symbols.push(DocumentSymbolItem {
                name,
                kind: kind.to_string(),
                line: index + 1,
                column,
                end_line: index + 1,
                end_column: line.chars().count().max(column),
            });
        }
    }
    symbols
}

fn symbol_from_line(line: &str) -> Option<(&'static str, String)> {
    if let Some(name) = line
        .strip_prefix("class ")
        .or_else(|| line.strip_prefix("struct "))
        .or_else(|| line.strip_prefix("interface "))
        .or_else(|| line.strip_prefix("enum "))
        .or_else(|| line.strip_prefix("type "))
    {
        return Some((
            "type",
            name.split(|ch: char| ch.is_whitespace() || ch == '{' || ch == '=')
                .next()
                .unwrap_or("")
                .to_string(),
        ));
    }
    if let Some(name) = line
        .strip_prefix("func ")
        .or_else(|| line.strip_prefix("fn "))
        .or_else(|| line.strip_prefix("def "))
        .or_else(|| line.strip_prefix("function "))
    {
        return Some((
            "function",
            name.split('(').next().unwrap_or("").trim().to_string(),
        ));
    }
    None
}

fn rank_definition_candidate(
    ctx: &LocalCodeNavContext,
    req: &NavPositionRequest,
    token: &str,
    hit: &LocalSearchHit,
) -> f64 {
    let lower = hit.text.to_lowercase();
    let token_lower = token.to_lowercase();
    let filename = Path::new(hit.relative_path.as_str())
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mut score = 0.0;
    if hit.relative_path == ctx.file_project_relative {
        score += 1.5;
    }
    if filename == token {
        score += 4.0;
    }
    if hit.relative_path == ctx.file_project_relative && hit.line == req.line {
        score -= 3.0;
    }
    for pattern in [
        format!("class {}", token_lower),
        format!("interface {}", token_lower),
        format!("enum {}", token_lower),
        format!("struct {}", token_lower),
        format!("type {} ", token_lower),
        format!("func {}", token_lower),
        format!("fn {}", token_lower),
        format!("def {}", token_lower),
        format!("function {}", token_lower),
        format!("const {} =", token_lower),
        format!("let {} =", token_lower),
        format!("var {} =", token_lower),
    ] {
        if lower.contains(pattern.as_str()) {
            score += 2.0;
        }
    }
    if lower.trim_start().starts_with(token_lower.as_str()) {
        score += 1.0;
    }
    score
}

fn is_request_location(
    ctx: &LocalCodeNavContext,
    req: &NavPositionRequest,
    location: &NavLocation,
) -> bool {
    location.relative_path == ctx.file_project_relative
        && location.line == req.line
        && location.column <= req.column
        && location.end_column >= req.column
}

fn line_contains_word(line: &str, token: &str) -> bool {
    token_column(line, token).is_some()
}

fn token_column(line: &str, token: &str) -> Option<usize> {
    let mut search_start = 0usize;
    while let Some(offset) = line[search_start..].find(token) {
        let start = search_start + offset;
        let end = start + token.len();
        let before = line[..start].chars().next_back();
        let after = line[end..].chars().next();
        if before.map(|ch| !is_token_char(ch)).unwrap_or(true)
            && after.map(|ch| !is_token_char(ch)).unwrap_or(true)
        {
            return Some(line[..start].chars().count() + 1);
        }
        search_start = end;
        if search_start >= line.len() {
            break;
        }
    }
    None
}

fn is_token_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

fn local_search_root(ctx: &LocalCodeNavContext) -> String {
    if ctx.root_relative.trim().is_empty() {
        ".".to_string()
    } else {
        ctx.root_relative.clone()
    }
}

fn project_relative_path(ctx: &LocalCodeNavContext, path: &str) -> Option<String> {
    if ctx.root_relative.is_empty() {
        return Some(path.to_string());
    }
    path.strip_prefix(format!("{}/", ctx.root_relative).as_str())
        .map(ToOwned::to_owned)
        .or_else(|| (path == ctx.root_relative).then(String::new))
}

fn connector_error_message(err: (axum::http::StatusCode, axum::Json<Value>)) -> String {
    let (status, axum::Json(value)) = err;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(|message| format!("{message} ({status})"))
        .unwrap_or_else(|| format!("{value} ({status})"))
}
