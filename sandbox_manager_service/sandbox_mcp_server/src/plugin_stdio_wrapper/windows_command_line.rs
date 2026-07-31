// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn build(command: &[u16], arguments: &[Vec<u16>]) -> Result<Vec<u16>, String> {
    if command.is_empty() || command.contains(&0) {
        return Err("Windows Plugin stdio command path contains NUL or is empty".to_string());
    }
    let mut line = Vec::new();
    append_quoted_argument(&mut line, command);
    for argument in arguments {
        if argument.contains(&0) {
            return Err("Windows Plugin stdio argument contains NUL".to_string());
        }
        line.push(b' ' as u16);
        append_quoted_argument(&mut line, argument.as_slice());
    }
    line.push(0);
    if line.len() > 32_767 {
        return Err("Windows Plugin stdio command line is too large".to_string());
    }
    Ok(line)
}

fn append_quoted_argument(output: &mut Vec<u16>, argument: &[u16]) {
    let quote = b'"' as u16;
    let backslash = b'\\' as u16;
    let needs_quotes = argument.is_empty()
        || argument
            .iter()
            .any(|unit| matches!(*unit, 9 | 10 | 11 | 12 | 13 | 32) || *unit == quote);
    if !needs_quotes {
        output.extend_from_slice(argument);
        return;
    }
    output.push(quote);
    let mut backslashes = 0_usize;
    for unit in argument {
        if *unit == backslash {
            backslashes += 1;
            continue;
        }
        if *unit == quote {
            output.extend(std::iter::repeat_n(backslash, backslashes * 2 + 1));
            output.push(quote);
        } else {
            output.extend(std::iter::repeat_n(backslash, backslashes));
            output.push(*unit);
        }
        backslashes = 0;
    }
    output.extend(std::iter::repeat_n(backslash, backslashes * 2));
    output.push(quote);
}

#[cfg(test)]
mod tests {
    use super::build;

    fn utf16(value: &str) -> Vec<u16> {
        value.encode_utf16().collect()
    }

    fn rendered(command: &str, arguments: &[&str]) -> String {
        let arguments = arguments
            .iter()
            .map(|argument| utf16(argument))
            .collect::<Vec<_>>();
        let mut line = build(utf16(command).as_slice(), arguments.as_slice())
            .expect("build Windows command line");
        assert_eq!(line.pop(), Some(0));
        String::from_utf16(&line).expect("decode Windows command line")
    }

    #[test]
    fn quotes_empty_whitespace_quotes_and_trailing_backslashes() {
        assert_eq!(rendered("hook.exe", &["alpha"]), "hook.exe alpha");
        assert_eq!(rendered("hook.exe", &[""]), "hook.exe \"\"");
        assert_eq!(
            rendered("C:\\Program Files\\hook.exe", &["a b"]),
            "\"C:\\Program Files\\hook.exe\" \"a b\""
        );
        assert_eq!(
            rendered("hook.exe", &["a\\\"b", "C:\\Program Files\\"]),
            "hook.exe \"a\\\\\\\"b\" \"C:\\Program Files\\\\\""
        );
    }

    #[test]
    fn rejects_nul_and_command_line_overflow() {
        assert!(build(&[b'h' as u16, 0], &[]).is_err());
        assert!(build(&[b'h' as u16], &[vec![0]]).is_err());
        assert!(build(&[b'h' as u16], &[vec![b'a' as u16; 32_767]]).is_err());
    }
}
