use super::super::*;

pub(crate) use crate::core::tool_io::text_result;

pub(crate) fn parse_string_array(value: Option<&Value>) -> Option<Vec<String>> {
    let arr = value.and_then(|v| v.as_array())?;
    let items = arr
        .iter()
        .filter_map(|item| item.as_str().map(|v| v.trim().to_string()))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

pub(crate) fn truncate_for_event(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_string();
    }

    chars.into_iter().take(max_chars).collect::<String>() + "…(truncated)"
}

pub(crate) fn optional_trimmed_string(args: &Value, field: &str) -> Option<String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{} is required", field))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{} is required", field));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn canonical_or_original(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(path.as_path()).unwrap_or(path)
}

pub(crate) fn map_status_to_job_state(status: &str) -> &'static str {
    match status {
        "ok" => "done",
        "cancelled" => "cancelled",
        _ => "error",
    }
}

pub(crate) fn serialize_agent(agent: &AgentSpec) -> Value {
    json!({
        "id": agent.id,
        "name": agent.name,
        "description": agent.description.clone().unwrap_or_default(),
        "category": agent.category.clone().unwrap_or_default(),
        "skills": agent.skills.clone().unwrap_or_default(),
    })
}

pub(crate) fn serialize_commands(commands: &[CommandSpec]) -> Vec<Value> {
    commands
        .iter()
        .map(|cmd| {
            json!({
                "id": cmd.id,
                "name": cmd.name.clone().unwrap_or_default(),
                "description": cmd.description.clone().unwrap_or_default(),
            })
        })
        .collect()
}

pub(crate) fn with_chatos(server_name: &str, tool: &str, payload: Value, status: &str) -> Value {
    let mut object = payload.as_object().cloned().unwrap_or_default();
    object.insert(
        "chatos".to_string(),
        json!({ "status": status, "server": server_name, "tool": tool }),
    );
    Value::Object(object)
}
