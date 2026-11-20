fn normalized_explicit_capability_selection(
    metadata: &serde_json::Value,
) -> Result<Option<serde_json::Value>, LocalAgentRuntimeError> {
    let Some(selection) = metadata.get("explicit_capability_selection") else {
        return Ok(None);
    };
    let values = selection.as_object().ok_or_else(|| {
        LocalAgentRuntimeError::InvalidRequest(
            "explicit_capability_selection must be an object".to_string(),
        )
    })?;
    let kind = values
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LocalAgentRuntimeError::InvalidRequest(
                "explicit_capability_selection.kind is required".to_string(),
            )
        })?;
    let display_name = values
        .get("display_name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 200)
        .ok_or_else(|| {
            LocalAgentRuntimeError::InvalidRequest(
                "explicit_capability_selection.display_name must be 1..=200 characters".to_string(),
            )
        })?;
    match kind {
        "plugin" => {
            let plugin_key = values
                .get("plugin_key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 256)
                .ok_or_else(|| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "plugin selection requires a 1..=256 character plugin_key".to_string(),
                    )
                })?;
            let component_key = values
                .get("component_key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 256)
                .ok_or_else(|| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "plugin selection requires a 1..=256 character component_key".to_string(),
                    )
                })?;
            Ok(Some(json!({
                "kind": "plugin",
                "plugin_key": plugin_key,
                "component_key": component_key,
                "display_name": display_name,
            })))
        }
        "requirement_survey" => {
            if values
                .get("plugin_key")
                .is_some_and(|value| !value.is_null())
            {
                return Err(LocalAgentRuntimeError::InvalidRequest(
                    "requirement_survey selection must not contain plugin_key".to_string(),
                ));
            }
            Ok(Some(json!({
                "kind": "requirement_survey",
                "display_name": display_name,
            })))
        }
        _ => Err(LocalAgentRuntimeError::InvalidRequest(format!(
            "unsupported explicit capability selection kind: {kind}"
        ))),
    }
}
