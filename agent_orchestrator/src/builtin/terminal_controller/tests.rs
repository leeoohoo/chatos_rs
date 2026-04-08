use std::path::Path;

use super::context::{build_input_payload, normalize_shell_input};

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

#[test]
fn build_input_payload_uses_shell_line_endings() {
    let root = if cfg!(windows) {
        Path::new(r"C:\repo\sandbox")
    } else {
        Path::new("/tmp/repo/sandbox")
    };

    let payload = build_input_payload(root, root, "echo hi");

    if cfg!(windows) {
        assert!(payload.contains("\r"));
        assert!(!payload.contains("\n"));
    } else {
        assert!(payload.contains("\n"));
    }
}
