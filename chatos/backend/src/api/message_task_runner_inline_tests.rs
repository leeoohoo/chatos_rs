#[cfg(test)]
mod retry_request_tests {
    use super::parse_retry_message_task_runner_run_request;

    #[test]
    fn empty_retry_body_uses_default_request() {
        let request = parse_retry_message_task_runner_run_request(b"")
            .expect("empty retry body must be accepted");
        assert!(request.retry_instruction.is_none());
    }

    #[test]
    fn retry_body_keeps_user_instruction() {
        let request = parse_retry_message_task_runner_run_request(
            r#"{"retry_instruction":"配置已经补齐","execution_service_id":"mdm-service"}"#
                .as_bytes(),
        )
        .expect("retry instruction body must be parsed");
        assert_eq!(request.retry_instruction.as_deref(), Some("配置已经补齐"));
        assert_eq!(request.execution_service_id.as_deref(), Some("mdm-service"));
    }
}
