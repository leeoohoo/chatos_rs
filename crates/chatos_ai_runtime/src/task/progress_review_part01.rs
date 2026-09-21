fn collect_confirmed_project_paths(value: &Value, output: &mut BTreeSet<String>) {
    if output.len() >= MAX_CONFIRMED_PROJECT_PATHS {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "path" | "file" | "filename" | "relative_path" | "changed_paths"
                ) {
                    collect_path_values(value, output);
                } else if value.is_object() || value.is_array() {
                    collect_confirmed_project_paths(value, output);
                }
                if output.len() >= MAX_CONFIRMED_PROJECT_PATHS {
                    break;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_confirmed_project_paths(item, output);
                if output.len() >= MAX_CONFIRMED_PROJECT_PATHS {
                    break;
                }
            }
        }
        _ => {}
    }
}

fn collect_path_values(value: &Value, output: &mut BTreeSet<String>) {
    match value {
        Value::String(path) => {
            if let Some(path) = normalize_confirmed_project_path(path) {
                output.insert(path);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_path_values(item, output);
            }
        }
        Value::Object(_) => collect_confirmed_project_paths(value, output),
        _ => {}
    }
}

fn normalize_confirmed_project_path(path: &str) -> Option<String> {
    let normalized = path
        .trim()
        .trim_matches('`')
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.chars().count() > 512
        || normalized.as_bytes().get(1) == Some(&b':')
        || normalized
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        || !project_path_is_meaningful_progress(normalized.as_str())
    {
        return None;
    }
    Some(normalized)
}

fn tool_name_ends_with_any(name: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| name.ends_with(suffix))
}

fn collect_tool_result_error_text(value: &Value, output: &mut String) {
    match value {
        Value::String(text) => {
            output.push(' ');
            output.push_str(text);
        }
        Value::Array(items) => {
            for item in items {
                collect_tool_result_error_text(item, output);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "content"
                        | "result"
                        | "error"
                        | "message"
                        | "detail"
                        | "details"
                        | "path"
                        | "file"
                        | "filename"
                ) {
                    collect_tool_result_error_text(value, output);
                }
            }
        }
        _ => {}
    }
}

fn write_result_has_meaningful_project_path(payload: &Value) -> bool {
    let parsed_content = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok());
    payload
        .get("result")
        .into_iter()
        .chain(parsed_content.as_ref())
        .any(value_contains_meaningful_project_path)
}

fn value_contains_meaningful_project_path(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            if key == "path" {
                return value
                    .as_str()
                    .is_some_and(project_path_is_meaningful_progress);
            }
            value_contains_meaningful_project_path(value)
        }),
        Value::Array(items) => items.iter().any(value_contains_meaningful_project_path),
        _ => false,
    }
}

fn value_contains_placeholder_progress_path(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            if key == "path" {
                return value
                    .as_str()
                    .is_some_and(|path| !project_path_is_meaningful_progress(path));
            }
            value_contains_placeholder_progress_path(value)
        }),
        Value::Array(items) => items.iter().any(value_contains_placeholder_progress_path),
        _ => false,
    }
}

fn project_path_is_meaningful_progress(path: &str) -> bool {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty() {
        return false;
    }
    let components = normalized
        .trim_start_matches("./")
        .split('/')
        .filter(|component| !component.is_empty());
    !components
        .into_iter()
        .any(project_path_component_is_non_engineering_progress)
}

fn project_path_component_is_non_engineering_progress(component: &str) -> bool {
    let normalized = component.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        ".chatos" | ".git" | ".cache" | "node_modules" | "target" | "target-shared"
    ) {
        return true;
    }
    [
        "progress-guard",
        "inspection-unlock",
        "read-unlock",
        "unblock",
        "unlock",
        "restore",
        "enable-tools",
        "enable_tools",
        "resume-tools",
        "resume_tools",
        "placeholder",
        "sentinel",
        "probe",
        "task-runner-notes",
        "task_runner_notes",
        "task-runner-progress",
        "task_runner_progress",
        "task_runner_progress_note",
        "task-runner-progress-note",
        "execution-notes",
        "execution_notes",
        "inspection-note",
        "inspection_note",
        "progress-note",
        "progress_note",
        "执行记录",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
        || [
            ["task_runner", "temp"],
            ["task-runner", "temp"],
            ["task_runner", "notes"],
            ["task-runner", "notes"],
            ["temp", "restore"],
        ]
        .iter()
        .any(|markers| markers.iter().all(|marker| normalized.contains(marker)))
}

fn terminal_result_command(payload: &Value) -> String {
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parsed = serde_json::from_str::<Value>(content).ok();
    parsed
        .as_ref()
        .map(chatos_mcp_runtime::structured_result_payload)
        .and_then(|value| value.get("common"))
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("result")
                .map(chatos_mcp_runtime::structured_result_payload)
                .and_then(|value| value.get("common"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn terminal_result_exit_succeeded(payload: &Value) -> bool {
    terminal_result_exit_code(payload) == Some(0)
}

fn terminal_result_exit_code(payload: &Value) -> Option<i64> {
    let direct_exit_code = payload
        .get("result")
        .map(chatos_mcp_runtime::structured_result_payload)
        .and_then(|result| result.get("exit_code"))
        .and_then(Value::as_i64);
    let content_exit_code = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .map(|content| chatos_mcp_runtime::structured_result_payload(&content).clone())
        .and_then(|content| content.get("exit_code").and_then(Value::as_i64));
    direct_exit_code.or(content_exit_code)
}

fn terminal_result_has_validation_command(payload: &Value) -> bool {
    let command = terminal_result_command(payload);
    terminal_command_is_validation(command.as_str())
}

fn terminal_command_is_validation(command: &str) -> bool {
    [
        "cargo test",
        "cargo check",
        "cargo clippy",
        "pytest",
        "python -m unittest",
        "npm test",
        "npm run test",
        "npm run typecheck",
        "npm run build",
        "npm ci",
        "npm install",
        "npm audit",
        "pnpm test",
        "pnpm run test",
        "pnpm run typecheck",
        "pnpm build",
        "pnpm install",
        "pnpm audit",
        "yarn test",
        "yarn run test",
        "yarn run typecheck",
        "yarn build",
        "yarn install",
        "yarn audit",
        "go test",
        "mvn test",
        "mvn clean test",
        "mvn verify",
        "mvn clean verify",
        "mvn package",
        "mvn clean package",
        "./mvnw test",
        "./mvnw clean test",
        "./mvnw verify",
        "./mvnw clean verify",
        "./mvnw package",
        "./mvnw clean package",
        "gradle test",
        "gradle check",
        "gradle build",
        "./gradlew test",
        "./gradlew check",
        "./gradlew build",
        "dotnet test",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn terminal_result_process_id(payload: &Value) -> Option<String> {
    terminal_result_field(payload, "process_id")
        .and_then(|value| value.as_str().map(str::trim).map(ToString::to_string))
        .filter(|value| !value.is_empty())
}

fn terminal_result_is_busy(payload: &Value) -> bool {
    terminal_result_field(payload, "busy").and_then(|value| value.as_bool()) == Some(true)
}

fn tool_result_field(payload: &Value, field: &str) -> Option<Value> {
    payload
        .get("result")
        .map(chatos_mcp_runtime::structured_result_payload)
        .and_then(|result| result.get(field))
        .cloned()
        .or_else(|| {
            payload
                .get("content")
                .and_then(Value::as_str)
                .and_then(|content| serde_json::from_str::<Value>(content).ok())
                .and_then(|content| {
                    chatos_mcp_runtime::structured_result_payload(&content)
                        .get(field)
                        .cloned()
                })
        })
}

fn terminal_result_field(payload: &Value, field: &str) -> Option<Value> {
    tool_result_field(payload, field)
}

#[cfg(test)]
include!("progress_review_inline_tests.rs");
