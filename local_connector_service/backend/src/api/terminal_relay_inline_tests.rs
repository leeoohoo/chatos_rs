#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_output_forwards_v2_sequence_metadata() {
        let payload = terminal_event_to_ws_payload(
            "terminal_output",
            &json!({ "data": "hello", "sequence": 42, "protocol_version": 2 }),
        )
        .expect("terminal output payload");

        assert_eq!(payload["type"], "output");
        assert_eq!(payload["data"], "hello");
        assert_eq!(payload["sequence"], 42);
        assert_eq!(payload["protocol_version"], 2);
    }

    #[test]
    fn terminal_snapshot_forwards_recovery_cursor_and_truncation() {
        let payload = terminal_event_to_ws_payload(
            "terminal_snapshot",
            &json!({
                "data": "tail",
                "base_sequence": 37,
                "sequence": 42,
                "truncated": true,
                "protocol_version": 2,
            }),
        )
        .expect("terminal snapshot payload");

        assert_eq!(payload["type"], "snapshot");
        assert_eq!(payload["base_sequence"], 37);
        assert_eq!(payload["sequence"], 42);
        assert_eq!(payload["truncated"], true);
        assert_eq!(payload["protocol_version"], 2);
    }
}
