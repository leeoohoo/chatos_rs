use super::types::{
    DocumentSymbolItem, DocumentSymbolsRequest, DocumentSymbolsResponse, NavLocation,
    NavLocationsResponse, NavPositionRequest, ProjectContext,
};
use crate::services::workspace_search::{
    search_text, TextSearchRequest, DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_RESULTS, DEFAULT_MAX_VISITS,
};
use std::fs;
use std::path::Path;

pub fn fallback_definition(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    provider: &str,
) -> Result<NavLocationsResponse, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(NavLocationsResponse {
            provider: provider.to_string(),
            language: ctx.language.clone(),
            mode: "heuristic".to_string(),
            token: None,
            locations: Vec::new(),
        });
    };

    let mut outcome = search_text(&TextSearchRequest {
        root: ctx.root.clone(),
        query: token.clone(),
        max_results: DEFAULT_MAX_RESULTS,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        max_visits: DEFAULT_MAX_VISITS,
        case_sensitive: true,
        whole_word: true,
    })?;

    if outcome.entries.is_empty() {
        outcome = search_text(&TextSearchRequest {
            root: ctx.root.clone(),
            query: token.clone(),
            max_results: DEFAULT_MAX_RESULTS,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_visits: DEFAULT_MAX_VISITS,
            case_sensitive: false,
            whole_word: true,
        })?;
    }

    let mut locations: Vec<NavLocation> = outcome
        .entries
        .into_iter()
        .map(|entry| {
            let score = rank_definition_candidate(
                ctx,
                req,
                &token,
                &entry.relative_path,
                entry.line,
                &entry.text,
            );
            NavLocation {
                path: entry.path,
                relative_path: entry.relative_path,
                line: entry.line,
                column: entry.column,
                end_line: entry.line,
                end_column: entry.column + token.chars().count().saturating_sub(1),
                preview: entry.text,
                score,
            }
        })
        .collect();

    locations.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.relative_path.cmp(&right.relative_path))
            .then(left.line.cmp(&right.line))
    });
    if locations.len() > 20 {
        locations.truncate(20);
    }

    Ok(NavLocationsResponse {
        provider: provider.to_string(),
        language: ctx.language.clone(),
        mode: "heuristic".to_string(),
        token: Some(token),
        locations,
    })
}

pub fn fallback_references(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    provider: &str,
) -> Result<NavLocationsResponse, String> {
    let token = extract_token_at_position(&ctx.file_path, req.line, req.column)?;
    let Some(token) = token else {
        return Ok(NavLocationsResponse {
            provider: provider.to_string(),
            language: ctx.language.clone(),
            mode: "text-search".to_string(),
            token: None,
            locations: Vec::new(),
        });
    };

    let outcome = search_text(&TextSearchRequest {
        root: ctx.root.clone(),
        query: token.clone(),
        max_results: DEFAULT_MAX_RESULTS,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        max_visits: DEFAULT_MAX_VISITS,
        case_sensitive: true,
        whole_word: true,
    })?;

    let mut locations: Vec<NavLocation> = outcome
        .entries
        .into_iter()
        .map(|entry| NavLocation {
            score: if entry.relative_path == ctx.relative_path {
                1.5
            } else {
                1.0
            },
            path: entry.path,
            relative_path: entry.relative_path,
            line: entry.line,
            column: entry.column,
            end_line: entry.line,
            end_column: entry.column + token.chars().count().saturating_sub(1),
            preview: entry.text,
        })
        .collect();

    locations.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
    });
    if locations.len() > 100 {
        locations.truncate(100);
    }

    Ok(NavLocationsResponse {
        provider: provider.to_string(),
        language: ctx.language.clone(),
        mode: "text-search".to_string(),
        token: Some(token),
        locations,
    })
}

pub fn fallback_document_symbols(
    ctx: &ProjectContext,
    _req: &DocumentSymbolsRequest,
    provider: &str,
) -> Result<DocumentSymbolsResponse, String> {
    let content = fs::read_to_string(&ctx.file_path).map_err(|err| err.to_string())?;
    let mut symbols = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let symbol = if let Some(name) = trimmed
            .strip_prefix("class ")
            .or_else(|| trimmed.strip_prefix("struct "))
            .or_else(|| trimmed.strip_prefix("interface "))
            .or_else(|| trimmed.strip_prefix("enum "))
        {
            Some((
                "type",
                name.split_whitespace().next().unwrap_or("").to_string(),
            ))
        } else if let Some(name) = trimmed.strip_prefix("fn ") {
            Some((
                "function",
                name.split('(').next().unwrap_or("").trim().to_string(),
            ))
        } else if let Some(name) = trimmed.strip_prefix("def ") {
            Some((
                "function",
                name.split('(').next().unwrap_or("").trim().to_string(),
            ))
        } else if let Some(name) = trimmed.strip_prefix("function ") {
            Some((
                "function",
                name.split('(').next().unwrap_or("").trim().to_string(),
            ))
        } else {
            None
        };

        if let Some((kind, name)) = symbol {
            if !name.is_empty() {
                let column = line
                    .find(&name)
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
    }

    Ok(DocumentSymbolsResponse {
        provider: provider.to_string(),
        language: ctx.language.clone(),
        mode: "heuristic".to_string(),
        symbols,
    })
}

fn rank_definition_candidate(
    ctx: &ProjectContext,
    req: &NavPositionRequest,
    token: &str,
    relative_path: &str,
    line: usize,
    preview: &str,
) -> f64 {
    let lower = preview.to_lowercase();
    let token_lower = token.to_lowercase();
    let filename = Path::new(relative_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    let mut score = 0.0;
    if relative_path == ctx.relative_path {
        score += 1.5;
    }
    if filename == token {
        score += 4.0;
    }
    if relative_path == ctx.relative_path && line == req.line {
        score -= 3.0;
    }

    let definition_patterns = [
        format!("class {}", token_lower),
        format!("interface {}", token_lower),
        format!("enum {}", token_lower),
        format!("struct {}", token_lower),
        format!("trait {}", token_lower),
        format!("fn {}", token_lower),
        format!("def {}", token_lower),
        format!("function {}", token_lower),
        format!("const {} =", token_lower),
        format!("let {} =", token_lower),
        format!("var {} =", token_lower),
        format!("type {} ", token_lower),
        format!("impl {}", token_lower),
        format!("{}(", token_lower),
    ];

    for pattern in definition_patterns {
        if lower.contains(&pattern) {
            score += 2.0;
        }
    }

    if lower.starts_with(&token_lower) {
        score += 1.0;
    }

    score
}

pub(crate) fn extract_token_at_position(
    path: &Path,
    line: usize,
    column: usize,
) -> Result<Option<String>, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let lines: Vec<&str> = content.lines().collect();
    if line == 0 || line > lines.len() {
        return Ok(None);
    }

    let current = lines[line - 1];
    let chars: Vec<(usize, char)> = current.char_indices().collect();
    if chars.is_empty() {
        return Ok(None);
    }

    let mut index = column.saturating_sub(1).min(chars.len().saturating_sub(1));
    if !is_token_char(chars[index].1) && index > 0 && is_token_char(chars[index - 1].1) {
        index -= 1;
    }
    if !is_token_char(chars[index].1) {
        return Ok(None);
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

    Ok(Some(current[start_byte..end_byte].to_string()))
}

pub(crate) fn is_token_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

#[cfg(test)]
mod tests {
    use super::extract_token_at_position;
    use std::fs;
    use std::path::PathBuf;

    fn make_temp_file(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "code_nav_fallback_test_{}.txt",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, content).expect("write temp file");
        path
    }

    #[test]
    fn extract_token_supports_mid_word_column() {
        let path = make_temp_file("const helloWorld = 1;\n");
        let token = extract_token_at_position(&path, 1, 10).expect("extract token");
        assert_eq!(token.as_deref(), Some("helloWorld"));
        fs::remove_file(path).expect("cleanup file");
    }
}
