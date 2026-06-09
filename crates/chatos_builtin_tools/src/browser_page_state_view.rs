use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct BrowserInspectionStepsView {
    pub(crate) snapshot: String,
    pub(crate) console: String,
    pub(crate) vision: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BrowserConsoleStateView {
    pub(crate) total_messages: u64,
    pub(crate) total_errors: u64,
    pub(crate) has_message_count_by_type: bool,
}

pub(crate) fn browser_inspection_steps_view(
    response: &Value,
) -> Option<BrowserInspectionStepsView> {
    let steps = response.get("inspection_steps")?.as_object()?;
    Some(BrowserInspectionStepsView {
        snapshot: steps
            .get("snapshot")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
        console: steps
            .get("console")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
        vision: steps
            .get("vision")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
    })
}

pub(crate) fn browser_console_state_view(response: &Value) -> BrowserConsoleStateView {
    BrowserConsoleStateView {
        total_messages: response
            .get("total_messages")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        total_errors: response
            .get("total_errors")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        has_message_count_by_type: response.get("message_count_by_type").is_some(),
    }
}

pub(crate) fn has_console_observation(response: &Value) -> bool {
    let state = browser_console_state_view(response);
    if state.total_messages > 0 || state.total_errors > 0 || state.has_message_count_by_type {
        return true;
    }

    response
        .get("messages_brief")
        .and_then(|value| value.as_array())
        .is_some_and(|items| !items.is_empty())
        || response
            .get("errors_brief")
            .and_then(|value| value.as_array())
            .is_some_and(|items| !items.is_empty())
}
