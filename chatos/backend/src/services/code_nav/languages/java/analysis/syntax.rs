// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use once_cell::sync::Lazy;
use regex::Regex;

use crate::services::code_nav::languages::regex_utils::compile_static_regex;
use crate::services::code_nav::languages::shared_nav::last_identifier;

static FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(
        r"^\s*(?:@\w+(?:\([^)]*\))?\s*)*(?:(?:public|protected|private|static|final|transient|volatile)\s+)*(?:[\w.$\[\]<>?,]+\s+)+([A-Za-z_][A-Za-z0-9_]*)\s*(?:=[^;]*)?;\s*$",
    )
});

pub(crate) fn extract_method_signature(
    line: &str,
    current_type_name: &str,
) -> Option<(String, String)> {
    let line = strip_leading_java_annotations(line.trim());
    if line.is_empty() || matches_java_non_method_statement(line) {
        return None;
    }

    let open_paren = line.find('(')?;
    let before_params = line[..open_paren].trim_end();
    if let Some(close_end) = matching_java_paren_end(&line[open_paren..]) {
        let after_params = line.get(open_paren + close_end..).unwrap_or("");
        if !is_java_method_suffix(after_params) {
            return None;
        }
    }
    if before_params.contains('=') || before_params.contains("->") {
        return None;
    }

    let name = last_identifier(before_params)?;
    if matches!(
        name.as_str(),
        "if" | "for" | "while" | "switch" | "catch" | "return" | "throw" | "new"
    ) {
        return None;
    }

    let kind = if !current_type_name.is_empty() && name == current_type_name {
        "constructor"
    } else {
        let declaration_prefix = before_params.strip_suffix(name.as_str())?.trim_end();
        if declaration_prefix.ends_with('.') {
            return None;
        }
        if !has_java_method_return_type(declaration_prefix) {
            return None;
        }
        "method"
    };
    Some((name, kind.to_string()))
}

fn strip_leading_java_annotations(mut line: &str) -> &str {
    loop {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('@') {
            return trimmed;
        }
        let Some(rest) = consume_java_annotation(trimmed) else {
            return trimmed;
        };
        line = rest;
        if line.trim().is_empty() {
            return "";
        }
    }
}

fn consume_java_annotation(line: &str) -> Option<&str> {
    let mut index = '@'.len_utf8();
    let name = line.get(index..)?;
    let mut chars = name.char_indices();
    let (_, first) = chars.next()?;
    if !is_java_identifier_start(first) {
        return None;
    }
    index += first.len_utf8();

    for (offset, ch) in chars {
        if is_java_identifier_part(ch) || ch == '.' {
            index = '@'.len_utf8() + offset + ch.len_utf8();
        } else {
            break;
        }
    }

    let rest = line.get(index..)?.trim_start();
    if rest.starts_with('(') {
        let end = matching_java_paren_end(rest)?;
        return rest.get(end..);
    }
    Some(rest)
}

fn matching_java_paren_end(value: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_delim = '\0';
    let mut escaped = false;

    for (index, ch) in value.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == string_delim {
                in_string = false;
                string_delim = '\0';
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_string = true;
            string_delim = ch;
            continue;
        }

        if ch == '(' {
            depth += 1;
            continue;
        }
        if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(index + ch.len_utf8());
            }
        }
    }

    None
}

fn matches_java_non_method_statement(line: &str) -> bool {
    [
        "return ", "throw ", "package ", "import ", "if ", "for ", "while ", "switch ", "case ",
        "new ", "assert ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn is_java_method_suffix(value: &str) -> bool {
    let suffix = value.trim_start();
    suffix.is_empty()
        || suffix.starts_with('{')
        || suffix.starts_with(';')
        || suffix.starts_with("throws ")
        || suffix.starts_with("default ")
}

fn has_java_method_return_type(prefix: &str) -> bool {
    let mut rest = prefix.trim();
    while let Some((word, after_word)) = split_first_java_word(rest) {
        if !is_java_modifier(word) {
            break;
        }
        rest = after_word.trim_start();
    }

    if rest.starts_with('<') {
        if let Some(end) = matching_java_angle_end(rest) {
            rest = rest.get(end..).unwrap_or("").trim_start();
        }
    }

    !rest.is_empty()
}

fn split_first_java_word(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start();
    let mut end = None;
    for (index, ch) in value.char_indices() {
        if index == 0 && !is_java_identifier_start(ch) {
            return None;
        }
        if is_java_identifier_part(ch) {
            end = Some(index + ch.len_utf8());
        } else {
            break;
        }
    }
    let end = end?;
    Some((&value[..end], &value[end..]))
}

fn is_java_modifier(value: &str) -> bool {
    matches!(
        value,
        "public"
            | "protected"
            | "private"
            | "static"
            | "final"
            | "abstract"
            | "synchronized"
            | "native"
            | "default"
            | "strictfp"
    )
}

fn matching_java_angle_end(value: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (index, ch) in value.char_indices() {
        if ch == '<' {
            depth += 1;
            continue;
        }
        if ch == '>' {
            depth -= 1;
            if depth == 0 {
                return Some(index + ch.len_utf8());
            }
        }
    }
    None
}

fn is_java_identifier_start(value: char) -> bool {
    value == '_' || value == '$' || value.is_alphabetic()
}

fn is_java_identifier_part(value: char) -> bool {
    value == '_' || value == '$' || value.is_alphanumeric()
}

pub(crate) fn extract_field_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if line.contains('(') {
        return None;
    }
    if matches_java_non_field_statement(trimmed) {
        return None;
    }
    if let Some(capture) = FIELD_RE.captures(line) {
        return Some(capture.get(1)?.as_str().to_string());
    }

    if !trimmed.ends_with(';') {
        return None;
    }

    let declaration_head = trimmed
        .trim_end_matches(';')
        .split('=')
        .next()
        .unwrap_or("")
        .trim_end();
    last_identifier(declaration_head)
}

fn matches_java_non_field_statement(line: &str) -> bool {
    [
        "return ", "throw ", "package ", "import ", "if ", "for ", "while ", "switch ", "case ",
        "new ", "assert ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

pub(super) fn strip_java_comments(line: &str, in_block_comment: &mut bool) -> String {
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
            if current == '\\' {
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

        if current == '"' || current == '\'' {
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
