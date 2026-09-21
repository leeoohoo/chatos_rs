#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_bind_ticket_fits_wechat_scene_limit() {
        let ticket = generate_secret(16);
        assert_eq!(ticket.len(), 32);
        assert!(!ticket.contains(char::is_whitespace));
    }

    #[test]
    fn secret_validation_rejects_values_that_could_break_protocol_boundaries() {
        assert!(validate_secret("", "ticket").is_err());
        assert!(validate_secret("has spaces", "ticket").is_err());
        assert!(validate_secret(&"x".repeat(513), "ticket").is_err());
        assert!(validate_secret("safe-value", "ticket").is_ok());
    }

    #[test]
    fn companion_scope_is_distinct_from_full_user_service_scope() {
        assert_ne!(WECHAT_COMPANION_SCOPE, "user_service");
    }
}
