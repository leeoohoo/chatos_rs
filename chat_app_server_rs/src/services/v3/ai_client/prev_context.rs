pub(super) fn should_use_prev_id_for_next_turn(
    prefer_stateless: bool,
    can_use_prev_id: bool,
    has_next_response_id: bool,
) -> bool {
    !prefer_stateless && can_use_prev_id && has_next_response_id
}

pub(super) fn is_unsupported_previous_response_id_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("previous_response_id")
        && (message.contains("unsupported parameter") || message.contains("invalid parameter"))
}

pub(super) fn base_url_allows_prev(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();

    if url.contains("api.openai.com") {
        return true;
    }
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    if let Ok(value) = std::env::var("ALLOW_PREV_ID_FOR_PROXY") {
        let normalized = value.trim().to_lowercase();
        if normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on" {
            return true;
        }
    }

    false
}

pub(super) fn is_invalid_input_text_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("input_text")
        && (message.contains("invalid value") || message.contains("invalid_value"))
}

pub(super) fn is_missing_tool_call_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("no tool call found")
        && (message.contains("function call output") || message.contains("function_call_output"))
}

#[cfg(test)]
mod tests {
    use super::should_use_prev_id_for_next_turn;

    #[test]
    fn keeps_stateless_mode_when_prefer_stateless_enabled() {
        assert!(!should_use_prev_id_for_next_turn(true, true, true));
        assert!(!should_use_prev_id_for_next_turn(true, true, false));
    }

    #[test]
    fn allows_prev_id_when_stateful_and_response_id_exists() {
        assert!(should_use_prev_id_for_next_turn(false, true, true));
        assert!(!should_use_prev_id_for_next_turn(false, true, false));
        assert!(!should_use_prev_id_for_next_turn(false, false, true));
    }
}
