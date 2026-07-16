// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn bounded_buffer_preserves_monotonic_offsets_after_eviction() {
    let mut logs = TerminalLogBuffer::new(2);
    assert!(logs.append("stdout", "one", "t1"));
    assert!(logs.append("stdout", "two", "t2"));
    assert!(logs.append("stderr", "three", "t3"));

    assert_eq!(logs.len(), 2);
    assert_eq!(
        logs.select_json(Some(1), 10),
        vec![
            json!({"offset": 1, "kind": "stdout", "content": "two", "created_at": "t2"}),
            json!({"offset": 2, "kind": "stderr", "content": "three", "created_at": "t3"})
        ]
    );
}

#[test]
fn recent_and_offset_pagination_keep_chronological_order() {
    let mut logs = TerminalLogBuffer::new(10);
    for index in 0..4 {
        logs.append("stdout", format!("{index}"), format!("t{index}"));
    }

    let recent = logs.recent_json(2);
    assert_eq!(recent[0]["offset"], 2);
    assert_eq!(recent[1]["offset"], 3);
    let page = logs.select_json(Some(1), 2);
    assert_eq!(page[0]["offset"], 1);
    assert_eq!(page[1]["offset"], 2);
}

#[test]
fn capture_truncates_by_unicode_characters_from_the_tail() {
    let output = collect_output_from_texts(["ab", "中文", "cd"].into_iter(), 4);
    assert_eq!(output.text, "中文cd");
    assert_eq!(output.char_count, 6);
    assert!(output.truncated);
}

#[test]
fn empty_log_chunks_are_ignored() {
    let mut logs = TerminalLogBuffer::new(2);
    assert!(!logs.append("stdout", "", "t1"));
    assert!(logs.is_empty());
}

#[test]
fn terminal_paths_resolve_children_and_reject_parent_escape() {
    let root = tempfile::tempdir().expect("workspace root");
    let child = root.path().join("child");
    std::fs::create_dir_all(&child).expect("workspace child");
    let canonical_root = canonicalize_existing(root.path()).expect("canonical root");

    assert_eq!(
        resolve_target_path(&canonical_root, "child").expect("resolve child"),
        canonicalize_existing(&child).expect("canonical child")
    );
    assert!(matches!(
        resolve_target_path(&canonical_root, ".."),
        Err(TerminalPathError::EscapesWorkspace)
    ));
    assert_eq!(
        display_workspace_path(&canonical_root, &canonical_root.join("child")),
        "/workspace/child"
    );
}

#[test]
fn terminal_session_meta_transitions_to_exited_once() {
    let mut meta = TerminalSessionMeta::new(
        "terminal-1".to_string(),
        "/workspace".to_string(),
        Some("project-1".to_string()),
        Some("user-1".to_string()),
        "echo ok".to_string(),
        "started".to_string(),
    );
    assert!(!meta.is_exited());
    assert!(meta.mark_exited(Some(0), "finished".to_string()));
    assert!(meta.is_exited());
    assert_eq!(meta.last_active_at, "finished");
    assert!(!meta.mark_exited(Some(1), "later".to_string()));
    assert_eq!(meta.exit_code, Some(0));
}

#[test]
fn terminal_scope_matches_user_and_project_without_path_fallback() {
    let root = Path::new("/workspace");
    let meta = TerminalSessionMeta::new(
        "terminal-1".to_string(),
        "/outside".to_string(),
        Some("project-1".to_string()),
        Some("user-1".to_string()),
        "echo ok".to_string(),
        "started".to_string(),
    );
    assert!(meta.matches_scope(root, Some("project-1"), Some("user-1")));
    assert!(!meta.matches_scope(root, Some("project-2"), Some("user-1")));
    assert!(!meta.matches_scope(root, Some("project-1"), Some("user-2")));
    assert!(!meta.matches_scope(root, None, Some("user-1")));
}

#[test]
fn terminal_wait_helpers_preserve_bounds_and_status() {
    assert_eq!(terminal_wait_timeout_ms(1), MIN_TERMINAL_WAIT_TIMEOUT_MS);
    assert_eq!(
        terminal_wait_timeout_ms(u64::MAX),
        MAX_TERMINAL_WAIT_TIMEOUT_MS
    );
    let meta = TerminalSessionMeta::new(
        "terminal-1".to_string(),
        "/workspace".to_string(),
        None,
        None,
        "echo ok".to_string(),
        "started".to_string(),
    );
    let timed_out = TerminalWaitResult::timed_out(1_000, &meta);
    assert!(timed_out.busy);
    assert!(timed_out.timed_out);
    assert_eq!(TerminalWaitResult::exited(50, Some(0)).finished_by, "exit");
}

#[tokio::test]
async fn output_reader_emits_lossy_utf8_chunks() {
    let input = std::io::Cursor::new(b"hello".to_vec());
    let chunks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = chunks.clone();
    read_output_chunks(input, move |chunk| {
        let captured = captured.clone();
        async move {
            captured.lock().expect("capture lock").push(chunk);
        }
    })
    .await
    .expect("read chunks");
    assert_eq!(chunks.lock().expect("chunks lock").as_slice(), &["hello"]);
}

#[tokio::test]
async fn terminal_wait_returns_immediately_for_exited_session() {
    let mut meta = TerminalSessionMeta::new(
        "terminal-1".to_string(),
        "/workspace".to_string(),
        None,
        None,
        "true".to_string(),
        "started".to_string(),
    );
    meta.mark_exited(Some(0), "finished".to_string());
    let result = wait_for_terminal_session(1, || {
        let meta = meta.clone();
        async move { Ok::<_, ()>(meta) }
    })
    .await
    .expect("wait result");
    assert_eq!(result.finished_by, "exit");
    assert_eq!(result.exit_code, Some(0));
}
