// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use once_cell::sync::Lazy;
use regex::Regex;

use crate::services::code_nav::languages::regex_utils::compile_static_regex;

use super::GoImport;

static IMPORT_SINGLE_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(r#"^\s*import\s+(?:(?:([A-Za-z_][A-Za-z0-9_]*)|_|\.)\s+)?"([^"]+)""#)
});
static IMPORT_BLOCK_ITEM_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r#"^\s*(?:(?:([A-Za-z_][A-Za-z0-9_]*)|_|\.)\s+)?"([^"]+)""#));
static TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(r"^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)\s+(struct|interface)\b")
});
static TYPE_ALIAS_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)\b"));
static METHOD_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*func\s*\([^)]*\)\s*([A-Za-z_][A-Za-z0-9_]*)\s*\("));
static FUNCTION_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*func\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("));
static VAR_CONST_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*(var|const)\s+([A-Za-z_][A-Za-z0-9_]*)\b"));
static SHORT_VAR_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:="));

pub(super) fn classify_go_declaration(line: &str, token: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if let Some((name, kind)) = extract_go_type_declaration(trimmed) {
        if name == token {
            return Some(match kind.as_str() {
                "struct" => "struct",
                "interface" => "interface",
                _ => "type",
            });
        }
    }
    if extract_go_method_name(trimmed).as_deref() == Some(token) {
        return Some("method");
    }
    if extract_go_function_name(trimmed).as_deref() == Some(token) {
        return Some("function");
    }
    if let Some((name, kind)) = extract_go_top_level_binding(trimmed) {
        if name == token {
            return Some(if kind == "constant" {
                "constant"
            } else {
                "variable"
            });
        }
    }
    if extract_go_short_var_name(trimmed).as_deref() == Some(token) {
        return Some("variable");
    }
    None
}

pub(super) fn parse_go_single_import(line: &str) -> Option<GoImport> {
    let capture = IMPORT_SINGLE_RE.captures(line)?;
    Some(GoImport {
        path: capture.get(2)?.as_str().to_string(),
    })
}

pub(super) fn parse_go_import_block_item(line: &str) -> Option<GoImport> {
    let capture = IMPORT_BLOCK_ITEM_RE.captures(line)?;
    Some(GoImport {
        path: capture.get(2)?.as_str().to_string(),
    })
}

pub(super) fn extract_go_type_declaration(line: &str) -> Option<(String, String)> {
    if let Some(capture) = TYPE_RE.captures(line) {
        let name = capture.get(1)?.as_str().to_string();
        let kind = match capture.get(2).map(|item| item.as_str()) {
            Some("struct") => "struct",
            Some("interface") => "interface",
            _ => "type",
        };
        return Some((name, kind.to_string()));
    }
    let capture = TYPE_ALIAS_RE.captures(line)?;
    let name = capture.get(1)?.as_str().to_string();
    Some((name, "type".to_string()))
}

pub(super) fn extract_go_method_name(line: &str) -> Option<String> {
    METHOD_RE
        .captures(line)
        .and_then(|capture| capture.get(1).map(|item| item.as_str().to_string()))
}

pub(super) fn extract_go_function_name(line: &str) -> Option<String> {
    FUNCTION_RE
        .captures(line)
        .and_then(|capture| capture.get(1).map(|item| item.as_str().to_string()))
}

pub(super) fn extract_go_top_level_binding(line: &str) -> Option<(String, String)> {
    if matches_go_non_binding_statement(line) {
        return None;
    }
    let capture = VAR_CONST_RE.captures(line)?;
    let kind = if capture.get(1).map(|item| item.as_str()) == Some("const") {
        "constant"
    } else {
        "variable"
    };
    Some((capture.get(2)?.as_str().to_string(), kind.to_string()))
}

fn extract_go_short_var_name(line: &str) -> Option<String> {
    if matches_go_non_binding_statement(line) {
        return None;
    }
    SHORT_VAR_RE
        .captures(line)
        .and_then(|capture| capture.get(1).map(|item| item.as_str().to_string()))
}

fn matches_go_non_binding_statement(line: &str) -> bool {
    [
        "return ", "go ", "defer ", "if ", "for ", "switch ", "case ", "select ", "package ",
        "import ", "func ", "type ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

pub(super) fn strip_go_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    let mut in_string = false;
    let mut string_delim = '\0';

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if *in_block_comment {
            if current == '*' && next == Some('/') {
                *in_block_comment = false;
                out.push(' ');
                out.push(' ');
                index += 2;
            } else {
                out.push(' ');
                index += 1;
            }
            continue;
        }

        if in_string {
            out.push(current);
            if current == '\\' && string_delim != '`' {
                if let Some(next) = next {
                    out.push(next);
                    index += 2;
                    continue;
                }
            }
            if current == string_delim {
                in_string = false;
                string_delim = '\0';
            }
            index += 1;
            continue;
        }

        if matches!(current, '"' | '\'' | '`') {
            in_string = true;
            string_delim = current;
            out.push(current);
            index += 1;
            continue;
        }

        if current == '/' && next == Some('/') {
            while index < chars.len() {
                out.push(' ');
                index += 1;
            }
            break;
        }

        if current == '/' && next == Some('*') {
            *in_block_comment = true;
            out.push(' ');
            out.push(' ');
            index += 2;
            continue;
        }

        out.push(current);
        index += 1;
    }

    out
}
