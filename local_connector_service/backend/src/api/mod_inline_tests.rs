#[cfg(test)]
mod tests {
    use super::is_allowed_model_config_proxy_request;
    use axum::http::Method;

    #[test]
    fn model_provider_crud_and_refresh_are_available_to_native_clients() {
        assert!(is_allowed_model_config_proxy_request(
            &Method::GET,
            "/api/model-providers"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::POST,
            "/api/model-providers"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::PATCH,
            "/api/model-providers/provider-1"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::POST,
            "/api/model-providers/provider-1/refresh"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::DELETE,
            "/api/model-providers/provider-1"
        ));
        assert!(!is_allowed_model_config_proxy_request(
            &Method::PUT,
            "/api/model-providers/provider-1"
        ));
    }
}
