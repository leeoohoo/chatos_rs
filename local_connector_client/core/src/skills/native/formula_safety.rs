// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

const ALLOWED_FUNCTIONS: [&str; 12] = [
    "ABS", "AND", "AVERAGE", "COUNT", "COUNTA", "IF", "MAX", "MIN", "NOT", "OR", "ROUND", "SUM",
];

pub(in crate::skills::native) fn validate_local_formula_expression(
    expression: &str,
    is_plain_identifier_allowed: impl Fn(&str) -> bool,
) -> Result<()> {
    if !has_supported_formula_characters(expression) {
        return Err(anyhow!(
            "formula contains unsupported dynamic, string, or external-link syntax"
        ));
    }

    let bytes = expression.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\'' {
            cursor = validate_quoted_sheet_reference(expression, cursor)?;
            continue;
        }
        if starts_formula_identifier(bytes, cursor) {
            let start = cursor;
            cursor = identifier_end(bytes, cursor + 1);
            let lookahead = skip_spaces(bytes, cursor);
            let identifier = &expression[start..cursor];
            if bytes.get(lookahead) == Some(&b'(') {
                let function = identifier.to_ascii_uppercase();
                if !ALLOWED_FUNCTIONS.contains(&function.as_str()) {
                    return Err(anyhow!(
                        "formula function is not in the local safety allowlist: {function}"
                    ));
                }
                continue;
            }

            let is_sheet_reference = bytes.get(lookahead) == Some(&b'!');
            if is_sheet_reference {
                validate_formula_sheet_name(identifier)?;
            } else if !is_plain_identifier_allowed(identifier)
                && !is_numeric_exponent(bytes, identifier, start, lookahead)
            {
                return Err(anyhow!(
                    "formula named ranges are disabled; use cells, booleans, safe functions, or worksheet references"
                ));
            }
        } else {
            cursor += 1;
        }
    }
    Ok(())
}

fn has_supported_formula_characters(expression: &str) -> bool {
    expression.is_ascii()
        && expression.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'.'
                        | b'$'
                        | b':'
                        | b','
                        | b'+'
                        | b'-'
                        | b'*'
                        | b'/'
                        | b'%'
                        | b'^'
                        | b'<'
                        | b'>'
                        | b'='
                        | b'('
                        | b')'
                        | b'!'
                        | b'\''
                        | b' '
                )
        })
}

fn validate_quoted_sheet_reference(expression: &str, cursor: usize) -> Result<usize> {
    let bytes = expression.as_bytes();
    let end = expression[cursor + 1..]
        .find('\'')
        .map(|offset| cursor + 1 + offset)
        .ok_or_else(|| anyhow!("formula contains an unterminated worksheet name"))?;
    validate_formula_sheet_name(&expression[cursor + 1..end])?;
    if bytes.get(end + 1) != Some(&b'!') {
        return Err(anyhow!(
            "quoted formula identifiers are only allowed as worksheet references"
        ));
    }
    Ok(end + 2)
}

fn starts_formula_identifier(bytes: &[u8], cursor: usize) -> bool {
    bytes[cursor].is_ascii_alphabetic()
        || bytes[cursor] == b'_'
        || (bytes[cursor] == b'$' && bytes.get(cursor + 1).is_some_and(u8::is_ascii_alphabetic))
}

fn identifier_end(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len()
        && (bytes[cursor].is_ascii_alphanumeric() || matches!(bytes[cursor], b'_' | b'.' | b'$'))
    {
        cursor += 1;
    }
    cursor
}

fn skip_spaces(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() && bytes[cursor] == b' ' {
        cursor += 1;
    }
    cursor
}

fn is_numeric_exponent(bytes: &[u8], identifier: &str, start: usize, lookahead: usize) -> bool {
    let exponent_digit_index = if matches!(bytes.get(lookahead), Some(b'+' | b'-')) {
        lookahead + 1
    } else {
        lookahead
    };
    matches!(identifier, "E" | "e")
        && start > 0
        && bytes[start - 1].is_ascii_digit()
        && bytes
            .get(exponent_digit_index)
            .is_some_and(u8::is_ascii_digit)
}

fn validate_formula_sheet_name(value: &str) -> Result<()> {
    let characters = value.chars().count();
    if characters == 0
        || characters > 31
        || value.trim().is_empty()
        || value.starts_with('\'')
        || value.ends_with('\'')
        || value.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '\\' | '/' | '?' | '*' | '[' | ']')
        })
    {
        return Err(anyhow!("formula worksheet name is invalid"));
    }
    Ok(())
}
