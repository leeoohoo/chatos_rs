// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;
use std::path::Path;

pub(super) fn validate_file_mutation_contract(
    workspace_root: &Path,
    command: &str,
) -> Result<(), String> {
    let Some(mechanism) = detect_file_tool_bypass(workspace_root, command) else {
        return Ok(());
    };
    Err(serde_json::to_string(&json!({
        "category": "file_tool_required",
        "error": "Direct project file mutation through the terminal is not allowed",
        "command_executed": false,
        "mechanism": mechanism,
        "recovery": {
            "required_tool_family": "code_maintainer",
            "allowed_tools": [
                "write_file",
                "edit_file",
                "append_file",
                "delete_path",
                "apply_patch"
            ],
            "guidance": "Read the current target revision and perform the change with the matching file modification tool."
        }
    }))
    .unwrap_or_else(|_| "file_tool_required: direct terminal file mutation rejected".to_string()))
}

fn detect_file_tool_bypass(workspace_root: &Path, command: &str) -> Option<&'static str> {
    if let Some(script) = shell_wrapper_script(command) {
        if let Some(mechanism) = detect_file_tool_bypass(workspace_root, script) {
            return Some(mechanism);
        }
    }
    if let Some(mechanism) = direct_mutation_utility(workspace_root, command) {
        return Some(mechanism);
    }
    if workspace_redirection_target(workspace_root, command).is_some() {
        return Some("shell_redirection");
    }
    interpreter_file_mutation(command)
}

fn direct_mutation_utility(workspace_root: &Path, command: &str) -> Option<&'static str> {
    for segment in shell_segments(command) {
        let normalized = normalized_command_segment(segment);
        let tokens = normalized.split_whitespace().collect::<Vec<_>>();
        let executable = tokens.first().copied().unwrap_or_default();
        match executable {
            "git" if tokens.contains(&"apply") && !tokens.contains(&"--check") => {
                return Some("git_apply");
            }
            "patch" if !tokens.contains(&"--dry-run") => return Some("patch"),
            "apply_patch" => return Some("apply_patch"),
            "sed"
                if tokens.iter().skip(1).any(|token| {
                    token.starts_with('-') && token.trim_start_matches('-').contains('i')
                }) =>
            {
                return Some("sed_in_place");
            }
            "perl" | "ruby"
                if tokens.iter().skip(1).any(|token| {
                    token.starts_with('-') && token.trim_start_matches('-').contains('i')
                }) =>
            {
                return Some("interpreter_in_place_edit");
            }
            "tee" if tee_writes_workspace_target(workspace_root, tokens.as_slice()) => {
                return Some("tee");
            }
            "dd" if dd_writes_workspace_target(workspace_root, tokens.as_slice()) => {
                return Some("dd");
            }
            _ => {}
        }
    }
    None
}

fn interpreter_file_mutation(command: &str) -> Option<&'static str> {
    let normalized = normalized_command_segment(command);
    if let Some(mechanism) = normalized_interpreter_file_mutation(normalized.as_str()) {
        return Some(mechanism);
    }
    for segment in shell_segments(command) {
        let normalized = normalized_command_segment(segment);
        if let Some(mechanism) = normalized_interpreter_file_mutation(normalized.as_str()) {
            return Some(mechanism);
        }
    }
    None
}

fn normalized_interpreter_file_mutation(command: &str) -> Option<&'static str> {
    let executable = command.split_whitespace().next().unwrap_or_default();
    if matches!(executable, "python" | "python3" | "pypy" | "pypy3")
        && python_script_mutates_files(command)
    {
        return Some("python_file_write");
    }
    if executable == "node" && node_script_mutates_files(command) {
        return Some("node_file_write");
    }
    None
}

fn python_script_mutates_files(command: &str) -> bool {
    let compact = compact(command);
    [
        ".write_text(",
        ".write_bytes(",
        "os.remove(",
        "os.unlink(",
        "os.rename(",
        "os.replace(",
        "shutil.copy",
        "shutil.move",
        ".unlink(",
        ".rename(",
        ".replace(",
        ".mkdir(",
        ".touch(",
    ]
    .iter()
    .any(|needle| compact.contains(needle))
        || (compact.contains("open(")
            && [",'w'", ",\"w\"", ",'a'", ",\"a\"", ",'x'", ",\"x\""]
                .iter()
                .any(|mode| compact.contains(mode)))
}

fn node_script_mutates_files(command: &str) -> bool {
    let compact = compact(command);
    [
        "writefile(",
        "writefilesync(",
        "appendfile(",
        "appendfilesync(",
        "unlink(",
        "unlinksync(",
        "rm(",
        "rmsync(",
        "rename(",
        "renamesync(",
        "copyfile(",
        "copyfilesync(",
        "mkdir(",
        "mkdirsync(",
    ]
    .iter()
    .any(|needle| compact.contains(needle))
}

fn workspace_redirection_target(workspace_root: &Path, command: &str) -> Option<String> {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut escaped = false;
    let chars = command.char_indices().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if ch == '\\' && !single_quote {
            escaped = true;
            index += 1;
            continue;
        }
        if ch == '\'' && !double_quote {
            single_quote = !single_quote;
            index += 1;
            continue;
        }
        if ch == '"' && !single_quote {
            double_quote = !double_quote;
            index += 1;
            continue;
        }
        if ch != '>' || single_quote || double_quote {
            index += 1;
            continue;
        }

        let mut target_start = byte_index + ch.len_utf8();
        if command[target_start..].starts_with('>') {
            target_start += 1;
        }
        if command[target_start..].starts_with('|') {
            target_start += 1;
        }
        let Some(target) = parse_redirection_target(&command[target_start..]) else {
            index += 1;
            continue;
        };
        if target.starts_with('&') || target.starts_with('(') || safe_output_target(&target) {
            index += 1;
            continue;
        }
        if target_writes_workspace(workspace_root, target.as_str()) {
            return Some(target);
        }
        index += 1;
    }
    None
}

fn parse_redirection_target(input: &str) -> Option<String> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let first = input.chars().next()?;
    if first == '\'' || first == '"' {
        let value = input[first.len_utf8()..]
            .split(first)
            .next()
            .unwrap_or_default()
            .trim();
        return (!value.is_empty()).then(|| value.to_string());
    }
    let value = input
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ';' | '&' | '|'))
        .next()
        .unwrap_or_default()
        .trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn tee_writes_workspace_target(workspace_root: &Path, tokens: &[&str]) -> bool {
    tokens
        .iter()
        .skip(1)
        .filter(|token| !token.starts_with('-'))
        .any(|target| target_writes_workspace(workspace_root, target))
}

fn dd_writes_workspace_target(workspace_root: &Path, tokens: &[&str]) -> bool {
    tokens
        .iter()
        .skip(1)
        .filter_map(|token| token.strip_prefix("of="))
        .any(|target| target_writes_workspace(workspace_root, target))
}

fn target_writes_workspace(_workspace_root: &Path, raw_target: &str) -> bool {
    let target = raw_target.trim_matches(['\'', '"']);
    if safe_output_target(target) {
        return false;
    }
    let path = Path::new(target);
    if !path.is_absolute() {
        return true;
    }
    true
}

fn safe_output_target(target: &str) -> bool {
    matches!(target, "/dev/null" | "/dev/stdout" | "/dev/stderr")
}

fn shell_segments(command: &str) -> Vec<&str> {
    command
        .split([';', '\n'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .flat_map(|part| part.split('|'))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn normalized_command_segment(segment: &str) -> String {
    let mut tokens = segment.split_whitespace().peekable();
    if tokens.peek().copied() == Some("env") {
        tokens.next();
    }
    while tokens
        .peek()
        .copied()
        .is_some_and(is_environment_assignment)
    {
        tokens.next();
    }
    tokens.collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

fn shell_wrapper_script(command: &str) -> Option<&str> {
    let normalized = normalized_command_segment(command);
    let executable = normalized.split_whitespace().next()?;
    if !matches!(executable, "sh" | "bash" | "zsh" | "dash")
        && !["/sh", "/bash", "/zsh", "/dash"]
            .iter()
            .any(|suffix| executable.ends_with(suffix))
    {
        return None;
    }
    for marker in [" -c ", " -lc ", " -cl "] {
        let Some(index) = command.find(marker) else {
            continue;
        };
        let script = command[index + marker.len()..].trim();
        if script.len() >= 2 {
            let first = script.chars().next()?;
            let last = script.chars().last()?;
            if first == last && matches!(first, '\'' | '"') {
                return Some(&script[first.len_utf8()..script.len() - last.len_utf8()]);
            }
        }
        return Some(script);
    }
    None
}

fn is_environment_assignment(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && name
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
}

fn compact(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shell_redirection_to_workspace_files() {
        let root = std::env::temp_dir();
        for command in [
            "cat template > src/main.rs",
            "printf '%s' value >> README.md",
            "cargo test | tee test-output.log",
        ] {
            let error = validate_file_mutation_contract(root.as_path(), command)
                .expect_err("workspace redirection must be rejected");
            assert!(error.contains("file_tool_required"));
        }
    }

    #[test]
    fn allows_non_file_redirection_and_read_only_patch_checks() {
        let root = std::env::temp_dir();
        for command in [
            "cargo test >/dev/null 2>&1",
            "git apply --check change.patch",
            "patch --dry-run -p1 < change.patch",
        ] {
            assert!(validate_file_mutation_contract(root.as_path(), command).is_ok());
        }
    }

    #[test]
    fn rejects_python_and_node_file_writes() {
        let root = std::env::temp_dir();
        for command in [
            "python3 -c \"from pathlib import Path; Path('a').write_text('x')\"",
            "python -c \"open('a', 'w').write('x')\"",
            "node -e \"require('fs').writeFileSync('a', 'x')\"",
        ] {
            let error = validate_file_mutation_contract(root.as_path(), command)
                .expect_err("interpreter file writes must be rejected");
            assert!(error.contains("file_tool_required"));
        }
    }

    #[test]
    fn shell_wrappers_do_not_bypass_file_mutation_contract() {
        let root = std::env::temp_dir();
        for command in [
            "bash -lc 'echo value > src/main.rs'",
            "sh -c \"python3 -c 'from pathlib import Path; Path(\\\"a\\\").write_text(\\\"x\\\")'\"",
        ] {
            let error = validate_file_mutation_contract(root.as_path(), command)
                .expect_err("shell-wrapped file writes must be rejected");
            assert!(error.contains("file_tool_required"));
        }
    }

    #[test]
    fn allows_interpreter_validation_and_package_installation() {
        let root = std::env::temp_dir();
        for command in [
            "python -m pytest",
            "node --test",
            "npm ci --ignore-scripts",
            "pnpm install --frozen-lockfile --ignore-scripts",
        ] {
            assert!(validate_file_mutation_contract(root.as_path(), command).is_ok());
        }
    }

    #[test]
    fn rejects_in_place_edit_and_patch_utilities() {
        let root = std::env::temp_dir();
        for command in [
            "sed -i '' 's/a/b/' src/main.rs",
            "perl -pi -e 's/a/b/' src/main.rs",
            "git apply change.patch",
            "git -C workspace apply change.patch",
            "patch -p1 < change.patch",
            "dd if=template of=src/main.rs",
        ] {
            let error = validate_file_mutation_contract(root.as_path(), command)
                .expect_err("direct mutation utilities must be rejected");
            assert!(error.contains("file_tool_required"));
        }
    }
}
