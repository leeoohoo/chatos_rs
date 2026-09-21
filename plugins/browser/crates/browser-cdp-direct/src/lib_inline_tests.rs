#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_patterns_are_anchored_when_no_outer_star_is_present() {
        assert!(wildcard_match("*/api/*", "https://example.com/api/data"));
        assert!(wildcard_match(
            "https://example.com/*",
            "https://example.com/a"
        ));
        assert!(!wildcard_match(
            "https://example.com/*",
            "xhttps://example.com/a"
        ));
        assert!(!wildcard_match(
            "*/api/data",
            "https://example.com/api/data/extra"
        ));
    }

    #[tokio::test]
    async fn event_queue_polls_by_monotonic_sequence() {
        let queue = BoundedEventQueue::default();
        queue
            .push(
                "Runtime.consoleAPICalled".into(),
                serde_json::json!({"value": 1}),
            )
            .await;
        queue
            .push(
                "Runtime.consoleAPICalled".into(),
                serde_json::json!({"value": 2}),
            )
            .await;
        let first = queue.poll(0, 1, Duration::ZERO).await;
        assert_eq!(first.events.len(), 1);
        assert_eq!(first.events[0].sequence, 1);
        let second = queue.poll(1, 10, Duration::ZERO).await;
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].sequence, 2);
        assert_eq!(second.latest_sequence, 2);
    }
}
