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
    fn generated_desktop_login_ticket_is_distinct_and_fits_wechat_scene_limit() {
        let ticket = format!("{WECHAT_DESKTOP_LOGIN_SCENE_PREFIX}{}", generate_secret(15));
        assert_eq!(ticket.len(), 32);
        assert!(validate_desktop_login_scene(ticket.as_str()).is_ok());
        assert!(validate_desktop_login_scene(generate_secret(16).as_str()).is_err());
    }

    #[test]
    fn desktop_device_name_rejects_hidden_or_oversized_values() {
        assert_eq!(
            validate_desktop_device_name(None).unwrap(),
            "ChatOS Desktop"
        );
        assert!(validate_desktop_device_name(Some("Mac\nInjected")).is_err());
        assert!(validate_desktop_device_name(Some(&"x".repeat(81))).is_err());
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
