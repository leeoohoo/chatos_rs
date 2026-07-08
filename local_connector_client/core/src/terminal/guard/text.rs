// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(crate) fn normalize_terminal_input(data: &str) -> String {
    if cfg!(windows) {
        data.replace("\r\n", "\r").replace('\n', "\r")
    } else {
        data.to_string()
    }
}

pub(crate) fn sanitize_terminal_command_line(command_line: &str) -> String {
    strip_terminal_ansi(command_line)
        .chars()
        .filter(|ch| !ch.is_control())
        .collect()
}

pub(crate) fn clear_terminal_input_line(command_line: &str) -> String {
    let mut seq = String::new();
    for _ in command_line.chars() {
        seq.push('\u{8}');
        seq.push(' ');
        seq.push('\u{8}');
    }
    seq
}

fn strip_terminal_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                let _ = chars.next();
                for marker in chars.by_ref() {
                    if ('@'..='~').contains(&marker) {
                        break;
                    }
                }
            }
            Some(']') => {
                let _ = chars.next();
                let mut previous_escape = false;
                for marker in chars.by_ref() {
                    if marker == '\u{7}' || (previous_escape && marker == '\\') {
                        break;
                    }
                    previous_escape = marker == '\u{1b}';
                }
            }
            Some(_) => {
                let _ = chars.next();
            }
            None => {}
        }
    }
    out
}
