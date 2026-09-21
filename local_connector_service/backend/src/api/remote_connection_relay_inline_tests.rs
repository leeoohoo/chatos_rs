#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_object_connection_payloads() {
        let request = RemoteConnectionTestRelayRequest {
            workspace_id: Some("workspace-1".to_string()),
            connection: Some(json!("not-an-object")),
            verification_code: None,
        };

        assert!(!request.connection.is_some_and(|value| value.is_object()));
    }
}
