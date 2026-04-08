use std::path::Path;

use super::directory_guard::{
    build_return_to_root_command, normalize_shell_input, sanitize_command_line_for_guard,
    validate_directory_change_command,
};
use super::input_triggers_busy;
use super::path_utils::{normalize_path_for_compare, path_is_within_root};
use super::prompt_parser::{
    extract_prompt_cwd, infer_prompt_cwd_from_context, is_prompt_line, strip_ansi,
};

#[test]
fn input_only_marks_busy_when_it_can_change_foreground_command() {
    assert!(!input_triggers_busy(""));
    assert!(!input_triggers_busy("n"));
    assert!(!input_triggers_busy("\u{7f}"));
    assert!(input_triggers_busy("\r"));
    assert!(input_triggers_busy("npm run dev\r"));
    assert!(input_triggers_busy("\n"));
    assert!(input_triggers_busy("\u{3}"));
    assert!(input_triggers_busy("\u{4}"));
    assert!(input_triggers_busy("\u{1a}"));
}

#[test]
fn prompt_detection_matches_common_shell_prompts() {
    assert!(is_prompt_line("PS C:\\repo>"));
    assert!(is_prompt_line("C:\\repo>"));
    assert!(is_prompt_line("user@host:~/repo$"));
    assert!(!is_prompt_line("vite v5 ready in 123 ms"));
}

#[test]
fn prompt_parser_handles_ansi_wrapped_prompt() {
    let cwd = std::env::current_dir().unwrap();
    let expected = std::fs::canonicalize(&cwd).unwrap();

    let plain_prompt = if cfg!(windows) {
        format!("PS {}>", cwd.display())
    } else {
        format!("{}$", cwd.display())
    };
    let wrapped = format!("\u{1b}]633;A\u{7}\u{1b}[32m{plain_prompt}\u{1b}[0m");
    let cleaned = strip_ansi(wrapped.as_str());

    let parsed = extract_prompt_cwd(cleaned.as_str()).unwrap();
    assert_eq!(
        normalize_path_for_compare(parsed.as_path()),
        normalize_path_for_compare(expected.as_path())
    );
}

#[test]
fn prompt_parser_recognizes_shell_working_directory() {
    let cwd = std::env::current_dir().unwrap();
    let expected = std::fs::canonicalize(&cwd).unwrap();

    if cfg!(windows) {
        let line = format!("PS {}>", cwd.display());
        let parsed = extract_prompt_cwd(line.as_str()).unwrap();
        assert_eq!(
            normalize_path_for_compare(parsed.as_path()),
            normalize_path_for_compare(expected.as_path())
        );
    } else {
        let line = format!("{}$", cwd.display());
        let parsed = extract_prompt_cwd(line.as_str()).unwrap();
        assert_eq!(
            normalize_path_for_compare(parsed.as_path()),
            normalize_path_for_compare(expected.as_path())
        );
    }
}

#[test]
fn directory_guard_allows_descendants_and_blocks_escape() {
    let unique = format!(
        "agent-orchestrator-terminal-guard-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let base = std::env::temp_dir().join(unique);
    let root = base.join("root");
    let child = root.join("child");

    std::fs::create_dir_all(&child).unwrap();

    let root = std::fs::canonicalize(&root).unwrap();
    let mut current = root.clone();

    assert!(validate_directory_change_command("cd child", root.as_path(), &mut current).is_none());
    assert!(path_is_within_root(current.as_path(), root.as_path()));

    assert!(validate_directory_change_command("cd ..", root.as_path(), &mut current).is_none());
    assert_eq!(current, root);

    let blocked_root_parent =
        validate_directory_change_command("cd ..", root.as_path(), &mut current);
    assert!(blocked_root_parent.is_some());

    let escape = format!("cd ..{}..", std::path::MAIN_SEPARATOR);
    let blocked = validate_directory_change_command(escape.as_str(), root.as_path(), &mut current);
    assert!(blocked.is_some());

    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn directory_guard_blocks_dynamic_cd_syntax() {
    let root = std::fs::canonicalize(".").unwrap();
    let mut current = root.clone();
    let blocked = validate_directory_change_command("cd $HOME", root.as_path(), &mut current);
    assert!(blocked.is_some());
}

#[test]
fn directory_guard_blocks_escape_when_cd_is_pasted_with_ansi_wrapper() {
    let unique = format!(
        "agent-orchestrator-terminal-guard-ansi-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let base = std::env::temp_dir().join(unique);
    let root = base.join("root");
    let outside = base.join("outside");

    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    let root = std::fs::canonicalize(&root).unwrap();
    let outside = std::fs::canonicalize(&outside).unwrap();
    let mut current = root.clone();

    let pasted = format!("\x1b[200~cd {}\x1b[201~", outside.display());
    let sanitized = sanitize_command_line_for_guard(pasted.as_str());
    assert_eq!(sanitized, format!("cd {}", outside.display()));

    let blocked =
        validate_directory_change_command(sanitized.as_str(), root.as_path(), &mut current);
    assert!(blocked.is_some());

    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn directory_guard_blocks_unresolvable_absolute_target() {
    let root = std::fs::canonicalize(".").unwrap();
    let mut current = root.clone();
    let candidate = format!(
        "/agent-orchestrator-restricted-terminal-unresolvable-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let command = format!("cd {candidate}");
    let blocked = validate_directory_change_command(command.as_str(), root.as_path(), &mut current);
    assert!(blocked.is_some());
}

#[test]
fn prompt_inference_handles_basename_prompts_within_root() {
    let unique = format!(
        "agent-orchestrator-terminal-prompt-infer-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let base = std::env::temp_dir().join(unique);
    let root = base.join("root");
    let child = root.join("child");

    std::fs::create_dir_all(&child).unwrap();

    let root = std::fs::canonicalize(&root).unwrap();
    let child = std::fs::canonicalize(&child).unwrap();

    let parsed_child =
        infer_prompt_cwd_from_context("(base) user@host child %", root.as_path(), root.as_path())
            .unwrap();
    assert_eq!(
        normalize_path_for_compare(parsed_child.as_path()),
        normalize_path_for_compare(child.as_path())
    );

    let root_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap()
        .to_string();
    let parent_prompt = format!("(base) user@host {root_name} %");
    let parsed_parent =
        infer_prompt_cwd_from_context(parent_prompt.as_str(), child.as_path(), root.as_path())
            .unwrap();
    assert_eq!(
        normalize_path_for_compare(parsed_parent.as_path()),
        normalize_path_for_compare(root.as_path())
    );

    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn prompt_inference_parses_home_tilde_prompt() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let expected = std::fs::canonicalize(home).unwrap();
    let guessed = infer_prompt_cwd_from_context(
        "(base) user@host ~ %",
        expected.as_path(),
        expected.as_path(),
    )
    .unwrap();

    assert_eq!(
        normalize_path_for_compare(guessed.as_path()),
        normalize_path_for_compare(expected.as_path())
    );
}

#[test]
fn sanitize_command_line_removes_tab_control_sequences() {
    let raw = "\x1b[200~cd /Users/lilei/\tproject\x1b[201~";
    let sanitized = sanitize_command_line_for_guard(raw);
    assert_eq!(sanitized, "cd /Users/lilei/project");
}

#[test]
fn build_return_to_root_command_navigates_back_to_root() {
    let root = if cfg!(windows) {
        Path::new("C:\\repo\\sandbox")
    } else {
        Path::new("/tmp/repo/sandbox")
    };
    let command = build_return_to_root_command(root);

    if cfg!(windows) {
        assert_eq!(command, "cd /d \"C:\\repo\\sandbox\"\r");
    } else {
        assert_eq!(command, "cd '/tmp/repo/sandbox'\n");
    }
}

#[test]
fn normalize_shell_input_uses_windows_return_key() {
    let raw = "echo one\necho two\r\necho three\r";
    let normalized = normalize_shell_input(raw);

    if cfg!(windows) {
        assert_eq!(normalized, "echo one\recho two\recho three\r");
    } else {
        assert_eq!(normalized, raw);
    }
}
