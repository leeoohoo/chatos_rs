// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use super::BasicSymbol;
use crate::services::code_nav::languages::shared_nav;

pub fn make_symbol(name: String, kind: &str, line: usize, column: usize) -> BasicSymbol {
    BasicSymbol {
        end_column: column + name.chars().count().saturating_sub(1),
        name,
        kind: kind.to_string(),
        line,
        column,
        end_line: line,
    }
}

pub fn find_column(line: &str, token: &str) -> Option<usize> {
    shared_nav::find_column(line, token)
}

pub fn last_identifier(value: &str) -> Option<String> {
    shared_nav::last_identifier(value)
}

pub fn strip_c_style_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    let mut in_string = false;
    let mut string_delim = '\0';
    let mut escaped = false;

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
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == string_delim {
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

pub fn count_char(value: &str, needle: char) -> usize {
    shared_nav::count_char(value, needle)
}

pub fn strip_leading_attributes(mut line: &str, open: char, close: char) -> &str {
    loop {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(open) {
            return trimmed;
        }
        let Some(end) = find_balanced_end(trimmed, open, close) else {
            return trimmed;
        };
        line = trimmed.get(end..).unwrap_or("");
        if line.trim().is_empty() {
            return "";
        }
    }
}

pub fn find_balanced_end(value: &str, open: char, close: char) -> Option<usize> {
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

        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(index + ch.len_utf8());
            }
        }
    }

    None
}

pub(super) fn extension_matches(path: &Path, extensions: &[&str]) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    extensions
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}
