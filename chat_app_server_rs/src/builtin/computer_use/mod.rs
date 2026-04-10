use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Map, Value};

use crate::core::tool_io::text_result;

const MAX_SCREENSHOT_INLINE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ALLOWED_INLINE_BYTES: usize = 10 * 1024 * 1024;
const POST_ACTION_OBSERVE_ATTEMPTS: usize = 3;
const POST_ACTION_OBSERVE_DELAY: Duration = Duration::from_millis(220);

#[derive(Debug, Clone)]
pub struct ComputerUseOptions {
    pub server_name: String,
    pub workspace_dir: String,
}

#[derive(Clone)]
pub struct ComputerUseService {
    tools: HashMap<String, Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

#[derive(Clone)]
struct BoundContext {
    server_name: String,
    workspace_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct ExecCandidate {
    program: String,
    prefix_args: Vec<String>,
    cwd: Option<PathBuf>,
    label: String,
}

#[derive(Debug, Clone)]
struct ToolOutput {
    command_line: String,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    cwd: Option<PathBuf>,
}

impl ComputerUseService {
    pub fn new(opts: ComputerUseOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
        };

        let bound = BoundContext {
            server_name: opts.server_name,
            workspace_dir: PathBuf::from(opts.workspace_dir),
        };

        service.register_command_entry(bound);
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                })
            })
            .collect()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args)
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.tools.insert(
            name.to_string(),
            Tool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
    }

    fn register_command_entry(&mut self, bound: BoundContext) {
        self.register_tool(
            "command",
            "Single entry for computer-use CLI. Provide one command string, for example: windows \"Safari\" --json, click \"Safari\" --button \"New Tab\" --json, type \"Safari\" --text \"hello world\" --enter --json, screenshot \"Safari\" --json.",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "max_inline_bytes": { "type": "integer", "minimum": 1024, "maximum": MAX_ALLOWED_INLINE_BYTES, "default": MAX_SCREENSHOT_INLINE_BYTES }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let obj = args_as_object(&args)?;
                let command = required_trimmed_string(obj, "command")?;
                let max_inline_bytes = optional_positive_u64(obj, "max_inline_bytes")
                    .map(|value| value as usize)
                    .unwrap_or(MAX_SCREENSHOT_INLINE_BYTES)
                    .clamp(1024, MAX_ALLOWED_INLINE_BYTES);

                let parsed = split_command_line(command.as_str())?;
                let cmd_args = normalize_command_args(parsed);
                if cmd_args.is_empty() {
                    return Err("command is empty after parsing".to_string());
                }

                execute_command_entry_tool("command", &bound, cmd_args.as_slice(), max_inline_bytes)
            }),
        );
    }
}

fn execute_command_entry_tool(
    tool_name: &str,
    ctx: &BoundContext,
    command_args: &[String],
    max_inline_bytes: usize,
) -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    if let Some(fallback_result) =
        maybe_execute_macos_open_url_command(tool_name, ctx, command_args)?
    {
        return Ok(fallback_result);
    }

    #[cfg(target_os = "macos")]
    if let Some(fallback_result) =
        maybe_execute_macos_browser_shortcut_command(tool_name, ctx, command_args)?
    {
        return Ok(fallback_result);
    }

    let output = run_computer_use_command(ctx, command_args)?;
    let parsed = parse_json_or_text(output.stdout.as_str());
    let result = if is_screenshot_command(command_args) {
        attach_screenshot_base64(parsed, max_inline_bytes, output.cwd.as_deref())
    } else {
        parsed
    };

    let mut response = json!({
        "tool": tool_name,
        "server": ctx.server_name,
        "command": output.command_line,
        "exit_code": output.exit_code,
        "result": result,
        "stderr": null_if_empty(output.stderr),
    });

    if let Some(post_observation) = maybe_collect_post_observation(ctx, command_args) {
        if let Value::Object(map) = &mut response {
            map.insert("post_observation".to_string(), post_observation);
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(browser_state) = maybe_collect_macos_browser_state_for_command(command_args) {
        if let Value::Object(map) = &mut response {
            map.insert("browser_state".to_string(), browser_state);
        }
    }

    Ok(text_result(response))
}

fn is_screenshot_command(command_args: &[String]) -> bool {
    command_args
        .first()
        .map(|value| value.eq_ignore_ascii_case("screenshot"))
        .unwrap_or(false)
}

fn maybe_collect_post_observation(ctx: &BoundContext, command_args: &[String]) -> Option<Value> {
    if !is_mutating_command(command_args) {
        return None;
    }

    let target = parse_command_target(command_args)?;
    let windows_args = build_windows_observe_args(&target)?;
    let mut last: Option<Value> = None;

    for attempt in 0..POST_ACTION_OBSERVE_ATTEMPTS {
        let output = match run_computer_use_command_raw(ctx, windows_args.as_slice()) {
            Ok(output) => output,
            Err(err) => {
                last = Some(json!({
                    "command": render_command_line("computer-use", &[], windows_args.as_slice()),
                    "error": err
                }));
                break;
            }
        };

        let parsed = parse_json_or_text(output.stdout.as_str());
        let success = output.exit_code == Some(0);
        let has_windows = parsed
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(!parsed.is_null());

        let mut observation = json!({
            "command": output.command_line,
            "exit_code": output.exit_code,
            "result": parsed,
            "stderr": null_if_empty(output.stderr),
        });

        #[cfg(target_os = "macos")]
        if let Some(browser_state) = maybe_collect_macos_browser_state_for_target(&target) {
            if let Value::Object(map) = &mut observation {
                map.insert("browser_state".to_string(), browser_state);
            }
        }

        last = Some(observation.clone());
        if success && has_windows {
            return Some(observation);
        }
        if attempt + 1 < POST_ACTION_OBSERVE_ATTEMPTS {
            thread::sleep(POST_ACTION_OBSERVE_DELAY);
        }
    }

    last
}

fn run_computer_use_command(ctx: &BoundContext, args: &[String]) -> Result<ToolOutput, String> {
    let output = run_computer_use_command_raw(ctx, args)?;
    if output.exit_code == Some(0) {
        return Ok(output);
    }
    Err(format_failed_tool_output(&output))
}

fn run_computer_use_command_raw(ctx: &BoundContext, args: &[String]) -> Result<ToolOutput, String> {
    let mut spawn_errors: Vec<String> = Vec::new();

    for candidate in computer_use_candidates(ctx) {
        let mut cmd = Command::new(&candidate.program);
        cmd.args(&candidate.prefix_args);
        cmd.args(args);
        if let Some(cwd) = candidate.cwd.as_ref() {
            cmd.current_dir(cwd);
        }

        let command_line = render_command_line(&candidate.program, &candidate.prefix_args, args);
        let output = match cmd.output() {
            Ok(output) => output,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                spawn_errors.push(format!(
                    "{} not found: {}",
                    candidate.label, candidate.program
                ));
                continue;
            }
            Err(err) => {
                spawn_errors.push(format!(
                    "failed to run {} ({}): {}",
                    candidate.program, candidate.label, err
                ));
                continue;
            }
        };

        return Ok(ToolOutput {
            command_line,
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            exit_code: output.status.code(),
            cwd: candidate.cwd.clone(),
        });
    }

    if spawn_errors.is_empty() {
        return Err("no computer-use execution candidates available".to_string());
    }

    Err(format!(
        "unable to locate runnable computer-use binary. tried:\n{}",
        spawn_errors.join("\n")
    ))
}

fn attach_screenshot_base64(
    parsed: Value,
    max_inline_bytes: usize,
    execution_cwd: Option<&Path>,
) -> Value {
    let mut obj = match parsed {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("raw_result".to_string(), other);
            map
        }
    };

    let output_path = obj
        .get("output_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let Some(output_path) = output_path else {
        obj.insert(
            "base64_status".to_string(),
            Value::String("skipped_missing_output_path".to_string()),
        );
        return Value::Object(obj);
    };

    let resolved_output_path = resolve_output_path(output_path, execution_cwd);
    obj.insert(
        "output_path_resolved".to_string(),
        Value::String(resolved_output_path.to_string_lossy().to_string()),
    );

    let file_meta = match std::fs::metadata(&resolved_output_path) {
        Ok(meta) => meta,
        Err(err) => {
            obj.insert(
                "base64_status".to_string(),
                Value::String("error_read_metadata".to_string()),
            );
            obj.insert("base64_detail".to_string(), Value::String(err.to_string()));
            return Value::Object(obj);
        }
    };

    let file_size = file_meta.len() as usize;
    obj.insert("image_bytes".to_string(), json!(file_size));
    obj.insert("max_inline_bytes".to_string(), json!(max_inline_bytes));

    if file_size == 0 {
        obj.insert(
            "base64_status".to_string(),
            Value::String("skipped_empty_file".to_string()),
        );
        return Value::Object(obj);
    }
    if file_size > max_inline_bytes {
        obj.insert(
            "base64_status".to_string(),
            Value::String("skipped_too_large".to_string()),
        );
        return Value::Object(obj);
    }

    let bytes = match std::fs::read(&resolved_output_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            obj.insert(
                "base64_status".to_string(),
                Value::String("error_read_file".to_string()),
            );
            obj.insert("base64_detail".to_string(), Value::String(err.to_string()));
            return Value::Object(obj);
        }
    };

    let mime = mime_guess::from_path(&resolved_output_path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string();
    let base64 = BASE64_STANDARD.encode(bytes);
    obj.insert("base64_status".to_string(), Value::String("ok".to_string()));
    obj.insert("image_mime".to_string(), Value::String(mime));
    obj.insert("image_base64".to_string(), Value::String(base64));
    Value::Object(obj)
}

fn resolve_output_path(path: &str, execution_cwd: Option<&Path>) -> PathBuf {
    let as_path = PathBuf::from(path);
    if as_path.is_absolute() {
        return as_path;
    }

    if let Some(cwd) = execution_cwd {
        return cwd.join(as_path);
    }

    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join(as_path);
    }

    as_path
}

fn format_failed_tool_output(output: &ToolOutput) -> String {
    let status = output
        .exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated by signal".to_string());

    let mut message = format!(
        "computer-use command failed (status: {})\ncmd: {}",
        status, output.command_line
    );
    if !output.stdout.is_empty() {
        message.push_str("\nstdout:\n");
        message.push_str(output.stdout.as_str());
    }
    if !output.stderr.is_empty() {
        message.push_str("\nstderr:\n");
        message.push_str(output.stderr.as_str());
    }
    message
}

fn parse_json_or_text(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return Value::Null;
    }
    serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

fn null_if_empty(value: String) -> Value {
    if value.trim().is_empty() {
        Value::Null
    } else {
        Value::String(value)
    }
}

fn args_as_object(args: &Value) -> Result<&Map<String, Value>, String> {
    args.as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())
}

fn required_trimmed_string(args: &Map<String, Value>, key: &str) -> Result<String, String> {
    optional_trimmed_string(args, key).ok_or_else(|| format!("missing required field: {key}"))
}

fn optional_trimmed_string(args: &Map<String, Value>, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn optional_positive_u64(args: &Map<String, Value>, key: &str) -> Option<u64> {
    args.get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTypeCommand {
    target: Option<String>,
    bundle_id: Option<String>,
    text: String,
    press_enter: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedClickCommand {
    target: Option<String>,
    bundle_id: Option<String>,
    button: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedCommandTarget {
    target: Option<String>,
    bundle_id: Option<String>,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserRuntimeState {
    frontmost_app: Option<String>,
    window_count: Option<u32>,
    tab_count: Option<u32>,
    active_tab_title: Option<String>,
    active_tab_url: Option<String>,
}

fn parse_type_command_args(command_args: &[String]) -> Option<ParsedTypeCommand> {
    let first = command_args.first()?.trim().to_ascii_lowercase();
    if first != "type" && first != "input" {
        return None;
    }
    if command_args.len() < 2 {
        return None;
    }

    let mut bundle_id: Option<String> = None;
    let mut text: Option<String> = None;
    let mut press_enter = false;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 1usize;
    while i < command_args.len() {
        match command_args[i].as_str() {
            "--bundle-id" | "-b" => {
                let value = command_args.get(i + 1)?;
                bundle_id = Some(normalize_cli_token(value));
                i += 2;
            }
            "--text" | "-t" => {
                let value = command_args.get(i + 1)?;
                text = Some(value.clone());
                i += 2;
            }
            "--enter" | "-e" => {
                press_enter = true;
                i += 1;
            }
            "--json" => {
                i += 1;
            }
            flag if flag.starts_with('-') => {
                i += 1;
            }
            _ => {
                positional.push(normalize_cli_token(&command_args[i]));
                i += 1;
            }
        }
    }

    let text = text?;
    if text.trim().is_empty() {
        return None;
    }
    if bundle_id.is_some() && !positional.is_empty() {
        return None;
    }
    if bundle_id.is_none() && positional.len() != 1 {
        return None;
    }

    Some(ParsedTypeCommand {
        target: positional.first().cloned(),
        bundle_id,
        text,
        press_enter,
    })
}

fn parse_click_command_args(command_args: &[String]) -> Option<ParsedClickCommand> {
    let first = command_args.first()?.trim().to_ascii_lowercase();
    if first != "click" {
        return None;
    }

    let mut bundle_id: Option<String> = None;
    let mut button: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 1usize;
    while i < command_args.len() {
        match command_args[i].as_str() {
            "--bundle-id" | "-b" => {
                let value = command_args.get(i + 1)?;
                bundle_id = Some(normalize_cli_token(value));
                i += 2;
            }
            "--button" | "-B" => {
                let value = command_args.get(i + 1)?;
                button = Some(value.clone());
                i += 2;
            }
            "--json" => {
                i += 1;
            }
            flag if flag.starts_with('-') => {
                i += 1;
            }
            _ => {
                positional.push(normalize_cli_token(&command_args[i]));
                i += 1;
            }
        }
    }

    let button = button?;
    if button.trim().is_empty() {
        return None;
    }
    if bundle_id.is_some() && !positional.is_empty() {
        return None;
    }
    if bundle_id.is_none() && positional.len() != 1 {
        return None;
    }

    Some(ParsedClickCommand {
        target: positional.first().cloned(),
        bundle_id,
        button,
    })
}

fn parse_command_target(command_args: &[String]) -> Option<ParsedCommandTarget> {
    let command = command_verb(command_args)?;
    if !matches!(
        command.as_str(),
        "open" | "windows" | "click" | "type" | "input" | "key" | "press" | "scroll" | "screenshot"
    ) {
        return None;
    }

    let mut bundle_id: Option<String> = None;
    let mut target: Option<String> = None;
    let mut i = 1usize;
    while i < command_args.len() {
        match command_args[i].as_str() {
            "--bundle-id" | "-b" => {
                let value = command_args.get(i + 1)?;
                bundle_id = Some(normalize_cli_token(value));
                i += 2;
            }
            "--text" | "-t" | "--button" | "-B" | "--key" | "-k" | "--times" | "-n"
            | "--direction" | "-d" | "--steps" | "--mode" | "-m" | "--window-title" | "-w"
            | "--window-id" | "-i" => {
                i += 2;
            }
            "--enter" | "-e" | "--json" => {
                i += 1;
            }
            flag if flag.starts_with('-') => {
                i += 1;
            }
            _ => {
                if target.is_none() {
                    target = Some(normalize_cli_token(&command_args[i]));
                }
                i += 1;
            }
        }
    }

    Some(ParsedCommandTarget { target, bundle_id })
}

fn command_verb(command_args: &[String]) -> Option<String> {
    command_args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn is_mutating_command(command_args: &[String]) -> bool {
    command_verb(command_args).is_some_and(|verb| {
        matches!(
            verb.as_str(),
            "open" | "click" | "type" | "input" | "key" | "press" | "scroll"
        )
    })
}

fn build_windows_observe_args(target: &ParsedCommandTarget) -> Option<Vec<String>> {
    if let Some(bundle_id) = target.bundle_id.as_deref().map(str::trim) {
        if !bundle_id.is_empty() {
            return Some(vec![
                "windows".to_string(),
                "--bundle-id".to_string(),
                bundle_id.to_string(),
                "--json".to_string(),
            ]);
        }
    }
    let app_target = target
        .target
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())?;
    Some(vec![
        "windows".to_string(),
        app_target.to_string(),
        "--json".to_string(),
    ])
}

fn normalize_cli_token(raw: &str) -> String {
    let mut current = raw.trim();
    loop {
        let trimmed = strip_surrounding_quotes_once(current);
        if trimmed == current {
            break;
        }
        current = trimmed.trim();
    }
    current.to_string()
}

fn strip_surrounding_quotes_once(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.len() < 2 {
        return trimmed;
    }

    let first = trimmed.chars().next().unwrap_or_default();
    let last = trimmed.chars().last().unwrap_or_default();
    let is_pair = matches!(
        (first, last),
        ('"', '"') | ('\'', '\'') | ('“', '”') | ('‘', '’')
    );
    if !is_pair {
        return trimmed;
    }

    let start = first.len_utf8();
    let end = trimmed.len().saturating_sub(last.len_utf8());
    &trimmed[start..end]
}

fn normalize_browser_app_name(app_name: &str) -> String {
    let token = normalize_cli_token(app_name);
    if token.ends_with(".app") {
        if let Some(stem) = Path::new(token.as_str())
            .file_stem()
            .and_then(|v| v.to_str())
        {
            return stem.trim().to_ascii_lowercase();
        }
    }
    token.trim().to_ascii_lowercase()
}

fn is_probably_browser_app_name(app_name: &str) -> bool {
    let normalized = normalize_browser_app_name(app_name);
    if normalized.is_empty() {
        return false;
    }
    matches!(
        normalized.as_str(),
        "safari"
            | "google chrome"
            | "chromium"
            | "arc"
            | "microsoft edge"
            | "firefox"
            | "brave browser"
            | "opera"
            | "vivaldi"
    ) || normalized.contains("chrome")
        || normalized.contains("safari")
        || normalized.contains("edge")
        || normalized.contains("firefox")
        || normalized.contains("brave")
        || normalized.contains("opera")
        || normalized.contains("vivaldi")
        || normalized.contains("arc")
}

fn is_new_tab_button_query(button: &str) -> bool {
    let normalized = button.trim().to_ascii_lowercase();
    normalized.contains("new tab")
        || normalized.contains("newtab")
        || normalized.contains("新标签")
        || normalized.contains("新標籤")
        || normalized.contains("标签页")
}

fn parse_open_url_command(command_args: &[String]) -> Option<String> {
    let command = command_verb(command_args)?;
    if command != "open" {
        return None;
    }

    let mut positional: Vec<String> = Vec::new();
    let mut i = 1usize;
    while i < command_args.len() {
        match command_args[i].as_str() {
            "--json" => i += 1,
            "--bundle-id" | "-b" => return None,
            flag if flag.starts_with('-') => i += 1,
            _ => {
                positional.push(normalize_cli_token(&command_args[i]));
                i += 1;
            }
        }
    }

    if positional.len() != 1 {
        return None;
    }
    let url = positional.remove(0);
    if is_http_url(url.as_str()) {
        Some(url)
    } else {
        None
    }
}

fn is_http_url(raw: &str) -> bool {
    let value = raw.trim().to_ascii_lowercase();
    value.starts_with("http://") || value.starts_with("https://")
}

#[cfg(target_os = "macos")]
fn maybe_execute_macos_open_url_command(
    tool_name: &str,
    ctx: &BoundContext,
    command_args: &[String],
) -> Result<Option<Value>, String> {
    let Some(url) = parse_open_url_command(command_args) else {
        return Ok(None);
    };

    run_macos_open_url(url.as_str())?;
    thread::sleep(Duration::from_millis(180));

    let frontmost_app = read_macos_frontmost_app_name().ok();
    let browser_state = frontmost_app.as_deref().and_then(|app_name| {
        if is_probably_browser_app_name(app_name) {
            read_macos_browser_runtime_state(app_name).ok()
        } else {
            None
        }
    });

    let mut result = json!({
        "url": url,
        "mode": "open_url_system",
        "frontmost_app": frontmost_app,
    });
    if let Some(state) = browser_state.as_ref() {
        if let Value::Object(map) = &mut result {
            map.insert(
                "browser_state".to_string(),
                browser_runtime_state_to_json(state),
            );
        }
    }

    Ok(Some(text_result(json!({
        "tool": tool_name,
        "server": ctx.server_name,
        "command": "builtin_macos_open_url",
        "exit_code": 0,
        "result": result,
        "stderr": Value::Null,
    }))))
}

#[cfg(target_os = "macos")]
fn run_macos_open_url(url: &str) -> Result<(), String> {
    let output = Command::new("open")
        .arg(url)
        .output()
        .map_err(|err| format!("failed to run open URL command: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!(
        "open URL command failed status={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr
    ))
}

#[cfg(target_os = "macos")]
fn read_macos_frontmost_app_name() -> Result<String, String> {
    let script = [
        "tell application \"System Events\"",
        "try",
        "return name of first process whose frontmost is true",
        "on error",
        "return \"\"",
        "end try",
        "end tell",
    ];
    let output = run_osascript_script(script.as_slice(), &[], "read frontmost app")?;
    let app_name = output.trim();
    if app_name.is_empty() {
        Err("frontmost app is empty".to_string())
    } else {
        Ok(app_name.to_string())
    }
}

#[cfg(target_os = "macos")]
fn maybe_execute_macos_browser_shortcut_command(
    tool_name: &str,
    ctx: &BoundContext,
    command_args: &[String],
) -> Result<Option<Value>, String> {
    if let Some(result) =
        maybe_execute_macos_browser_click_new_tab_command(tool_name, ctx, command_args)?
    {
        return Ok(Some(result));
    }
    maybe_execute_macos_browser_type_command(tool_name, ctx, command_args)
}

#[cfg(target_os = "macos")]
fn maybe_execute_macos_browser_click_new_tab_command(
    tool_name: &str,
    ctx: &BoundContext,
    command_args: &[String],
) -> Result<Option<Value>, String> {
    let Some(parsed) = parse_click_command_args(command_args) else {
        return Ok(None);
    };
    if !is_new_tab_button_query(parsed.button.as_str()) {
        return Ok(None);
    }

    let Some(app_name) = resolve_macos_browser_app_name_for_target(
        parsed.target.as_deref(),
        parsed.bundle_id.as_deref(),
    )?
    else {
        return Ok(None);
    };

    let before_state = read_macos_browser_runtime_state(app_name.as_str()).ok();
    run_macos_browser_new_tab_shortcut(app_name.as_str())?;
    let after_attempt = wait_for_macos_browser_state_change(
        app_name.as_str(),
        before_state.as_ref(),
        6,
        Duration::from_millis(200),
    )
    .ok();

    let mut result = json!({
        "app_name": app_name,
        "button": parsed.button,
        "mode": "browser_new_tab_shortcut",
    });
    if let Some(before_state) = before_state.as_ref() {
        if let Value::Object(map) = &mut result {
            map.insert(
                "before_state".to_string(),
                browser_runtime_state_to_json(before_state),
            );
        }
    }
    if let Some((after_state, changed)) = after_attempt {
        if let Value::Object(map) = &mut result {
            map.insert(
                "after_state".to_string(),
                browser_runtime_state_to_json(&after_state),
            );
            map.insert("state_changed".to_string(), Value::Bool(changed));
        }
    }

    let mut response = json!({
        "tool": tool_name,
        "server": ctx.server_name,
        "command": "builtin_macos_browser_new_tab_shortcut",
        "exit_code": 0,
        "result": result,
        "stderr": Value::Null,
    });

    if let Some(post_observation) = maybe_collect_post_observation(ctx, command_args) {
        if let Value::Object(map) = &mut response {
            map.insert("post_observation".to_string(), post_observation);
        }
    }
    if let Some(browser_state) = maybe_collect_macos_browser_state_for_command(command_args) {
        if let Value::Object(map) = &mut response {
            map.insert("browser_state".to_string(), browser_state);
        }
    }

    Ok(Some(text_result(response)))
}

#[cfg(target_os = "macos")]
fn maybe_execute_macos_browser_type_command(
    tool_name: &str,
    ctx: &BoundContext,
    command_args: &[String],
) -> Result<Option<Value>, String> {
    let Some(parsed) = parse_type_command_args(command_args) else {
        return Ok(None);
    };
    if !parsed.press_enter {
        return Ok(None);
    }

    let Some(app_name) = resolve_macos_browser_app_name_for_target(
        parsed.target.as_deref(),
        parsed.bundle_id.as_deref(),
    )?
    else {
        return Ok(None);
    };

    let before_state = read_macos_browser_runtime_state(app_name.as_str()).ok();
    run_macos_browser_address_bar_type(app_name.as_str(), parsed.text.as_str())?;

    let after_attempt = wait_for_macos_browser_state_change(
        app_name.as_str(),
        before_state.as_ref(),
        8,
        Duration::from_millis(220),
    )
    .ok();

    let mut result = json!({
        "app_name": app_name,
        "text": parsed.text,
        "press_enter": true,
        "mode": "browser_address_bar_shortcut",
    });
    if let Some(before_state) = before_state.as_ref() {
        if let Value::Object(map) = &mut result {
            map.insert(
                "before_state".to_string(),
                browser_runtime_state_to_json(before_state),
            );
        }
    }
    if let Some((after_state, changed)) = after_attempt {
        if let Value::Object(map) = &mut result {
            map.insert(
                "after_state".to_string(),
                browser_runtime_state_to_json(&after_state),
            );
            map.insert("state_changed".to_string(), Value::Bool(changed));
        }
    }

    let mut response = json!({
        "tool": tool_name,
        "server": ctx.server_name,
        "command": "builtin_macos_browser_type_shortcut",
        "exit_code": 0,
        "result": result,
        "stderr": Value::Null,
    });

    if let Some(post_observation) = maybe_collect_post_observation(ctx, command_args) {
        if let Value::Object(map) = &mut response {
            map.insert("post_observation".to_string(), post_observation);
        }
    }
    if let Some(browser_state) = maybe_collect_macos_browser_state_for_command(command_args) {
        if let Value::Object(map) = &mut response {
            map.insert("browser_state".to_string(), browser_state);
        }
    }

    Ok(Some(text_result(response)))
}

#[cfg(target_os = "macos")]
fn resolve_macos_browser_app_name_for_target(
    target: Option<&str>,
    bundle_id: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(bundle_id) = bundle_id.map(str::trim).filter(|value| !value.is_empty()) {
        let app_name = resolve_macos_app_name_by_bundle_id(bundle_id)?;
        if is_probably_browser_app_name(app_name.as_str()) {
            return Ok(Some(app_name));
        }
        return Ok(None);
    }

    let app_name = target
        .map(normalize_cli_token)
        .map(|value| {
            if value.ends_with(".app") {
                Path::new(value.as_str())
                    .file_stem()
                    .and_then(|item| item.to_str())
                    .map(str::to_string)
                    .unwrap_or(value)
            } else {
                value
            }
        })
        .unwrap_or_default();
    if app_name.trim().is_empty() || !is_probably_browser_app_name(app_name.as_str()) {
        return Ok(None);
    }
    Ok(Some(app_name))
}

#[cfg(target_os = "macos")]
fn maybe_collect_macos_browser_state_for_command(command_args: &[String]) -> Option<Value> {
    let target = parse_command_target(command_args)?;
    maybe_collect_macos_browser_state_for_target(&target)
}

#[cfg(target_os = "macos")]
fn maybe_collect_macos_browser_state_for_target(target: &ParsedCommandTarget) -> Option<Value> {
    let app_name = resolve_macos_browser_app_name_for_target(
        target.target.as_deref(),
        target.bundle_id.as_deref(),
    )
    .ok()
    .flatten()?;
    read_macos_browser_runtime_state(app_name.as_str())
        .ok()
        .map(|state| browser_runtime_state_to_json(&state))
}

#[cfg(target_os = "macos")]
fn read_macos_browser_runtime_state(app_name: &str) -> Result<BrowserRuntimeState, String> {
    let script = [
        "on replace_text(theText, searchString, replacementString)",
        "set AppleScript's text item delimiters to searchString",
        "set theItems to text items of theText",
        "set AppleScript's text item delimiters to replacementString",
        "set theText to theItems as string",
        "set AppleScript's text item delimiters to \"\"",
        "return theText",
        "end replace_text",
        "on run argv",
        "set appName to item 1 of argv",
        "set delim to \"|||\"",
        "set frontApp to \"\"",
        "set winCount to \"\"",
        "set tabCount to \"\"",
        "set tabTitle to \"\"",
        "set tabUrl to \"\"",
        "tell application \"System Events\"",
        "try",
        "set frontApp to name of first process whose frontmost is true",
        "end try",
        "end tell",
        "try",
        "tell application appName",
        "set winCount to (count of windows) as text",
        "if (count of windows) > 0 then",
        "try",
        "set tabCount to (count of tabs of front window) as text",
        "end try",
        "try",
        "set tabTitle to (title of active tab of front window) as text",
        "on error",
        "try",
        "set tabTitle to (name of current tab of front window) as text",
        "end try",
        "end try",
        "try",
        "set tabUrl to (URL of active tab of front window) as text",
        "on error",
        "try",
        "set tabUrl to (URL of current tab of front window) as text",
        "end try",
        "end try",
        "end if",
        "end tell",
        "end try",
        "set frontApp to my replace_text(frontApp, return, \" \")",
        "set frontApp to my replace_text(frontApp, delim, \" \")",
        "set tabTitle to my replace_text(tabTitle, return, \" \")",
        "set tabTitle to my replace_text(tabTitle, delim, \" \")",
        "set tabUrl to my replace_text(tabUrl, return, \" \")",
        "set tabUrl to my replace_text(tabUrl, delim, \" \")",
        "return frontApp & delim & winCount & delim & tabCount & delim & tabTitle & delim & tabUrl",
        "end run",
    ];
    let output =
        run_osascript_script(script.as_slice(), &[app_name], "read browser runtime state")?;
    let mut parts = output.splitn(5, "|||");
    let frontmost_app = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let window_count = parse_optional_u32(parts.next());
    let tab_count = parse_optional_u32(parts.next());
    let active_tab_title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let active_tab_url = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    Ok(BrowserRuntimeState {
        frontmost_app,
        window_count,
        tab_count,
        active_tab_title,
        active_tab_url,
    })
}

#[cfg(target_os = "macos")]
fn wait_for_macos_browser_state_change(
    app_name: &str,
    before_state: Option<&BrowserRuntimeState>,
    attempts: usize,
    delay: Duration,
) -> Result<(BrowserRuntimeState, bool), String> {
    let attempts = attempts.max(1);
    let mut last_state: Option<BrowserRuntimeState> = None;
    let mut last_error: Option<String> = None;

    for index in 0..attempts {
        match read_macos_browser_runtime_state(app_name) {
            Ok(state) => {
                let changed = before_state
                    .map(|before| has_browser_navigation_changed(before, &state))
                    .unwrap_or(false);
                if changed || index + 1 == attempts {
                    return Ok((state, changed));
                }
                last_state = Some(state);
            }
            Err(err) => last_error = Some(err),
        }
        if index + 1 < attempts {
            thread::sleep(delay);
        }
    }

    if let Some(state) = last_state {
        Ok((state, false))
    } else {
        Err(last_error.unwrap_or_else(|| "failed to read browser runtime state".to_string()))
    }
}

#[cfg(target_os = "macos")]
fn has_browser_navigation_changed(
    before: &BrowserRuntimeState,
    after: &BrowserRuntimeState,
) -> bool {
    if before.active_tab_url != after.active_tab_url {
        return true;
    }
    if before.active_tab_title != after.active_tab_title {
        return true;
    }
    if before.window_count != after.window_count {
        return true;
    }
    before.tab_count != after.tab_count
}

#[cfg(target_os = "macos")]
fn browser_runtime_state_to_json(state: &BrowserRuntimeState) -> Value {
    let frontmost_matches_target = state
        .frontmost_app
        .as_deref()
        .is_some_and(|frontmost| is_probably_browser_app_name(frontmost));
    json!({
        "frontmost_app": state.frontmost_app,
        "frontmost_app_is_browser": frontmost_matches_target,
        "window_count": state.window_count,
        "tab_count": state.tab_count,
        "active_tab_title": state.active_tab_title,
        "active_tab_url": state.active_tab_url,
    })
}

fn parse_optional_u32(raw: Option<&str>) -> Option<u32> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u32>().ok())
}

#[cfg(target_os = "macos")]
fn resolve_macos_app_name_by_bundle_id(bundle_id: &str) -> Result<String, String> {
    let script = [
        "on run argv",
        "set bundleId to item 1 of argv",
        "tell application id bundleId",
        "return name",
        "end tell",
        "end run",
    ];
    let output = run_osascript_script(script.as_slice(), &[bundle_id], "resolve app name")?;
    let app_name = output.trim();
    if app_name.is_empty() {
        Err(format!(
            "failed to resolve app name from bundle_id: {}",
            bundle_id
        ))
    } else {
        Ok(app_name.to_string())
    }
}

#[cfg(target_os = "macos")]
fn run_macos_browser_address_bar_type(app_name: &str, text: &str) -> Result<(), String> {
    let script = [
        "on run argv",
        "set appName to item 1 of argv",
        "set inputText to item 2 of argv",
        "tell application appName to activate",
        "delay 0.18",
        "tell application \"System Events\"",
        "if not (exists process appName) then error \"target process not found\"",
        "tell process appName",
        "set frontmost to true",
        "end tell",
        "delay 0.12",
        "keystroke \"l\" using command down",
        "delay 0.08",
        "keystroke inputText",
        "delay 0.1",
        "key code 36",
        "end tell",
        "return \"ok\"",
        "end run",
    ];
    let _ = run_osascript_script(
        script.as_slice(),
        &[app_name, text],
        "browser address bar type",
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_macos_browser_new_tab_shortcut(app_name: &str) -> Result<(), String> {
    let script = [
        "on run argv",
        "set appName to item 1 of argv",
        "tell application appName to activate",
        "delay 0.14",
        "tell application \"System Events\"",
        "if not (exists process appName) then error \"target process not found\"",
        "tell process appName",
        "set frontmost to true",
        "end tell",
        "delay 0.1",
        "keystroke \"t\" using command down",
        "end tell",
        "return \"ok\"",
        "end run",
    ];
    let _ = run_osascript_script(script.as_slice(), &[app_name], "browser new tab shortcut")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_osascript_script(
    script_lines: &[&str],
    args: &[&str],
    label: &str,
) -> Result<String, String> {
    let mut cmd = Command::new("osascript");
    for line in script_lines {
        cmd.arg("-e").arg(line);
    }
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd
        .output()
        .map_err(|err| format!("failed to run osascript ({label}): {err}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!(
        "osascript failed ({label}) status={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr
    ))
}

fn split_command_line(raw: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        match quote {
            Some(active_quote) => {
                if ch == active_quote {
                    quote = None;
                    continue;
                }
                if ch == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    } else {
                        current.push('\\');
                    }
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    } else {
                        current.push('\\');
                    }
                }
                value if value.is_whitespace() => {
                    if !current.is_empty() {
                        args.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            },
        }
    }

    if let Some(active_quote) = quote {
        return Err(format!("unclosed quote: {}", active_quote));
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn normalize_command_args(mut parsed: Vec<String>) -> Vec<String> {
    if parsed.is_empty() {
        return parsed;
    }

    let first = parsed[0].to_ascii_lowercase();
    if first == "cargo" && parsed.len() >= 3 {
        if let Some(marker_index) = parsed.iter().position(|value| value == "--") {
            return parsed.split_off(marker_index + 1);
        }
    }

    if first == "computer-use"
        || first == "computer-use-macos"
        || first == "computer-use-windows"
        || first.ends_with("/computer-use")
        || first.ends_with("\\computer-use.exe")
        || first.ends_with("/computer-use-macos")
        || first.ends_with("/computer-use-windows")
    {
        parsed.remove(0);
    }

    parsed
}

fn computer_use_candidates(ctx: &BoundContext) -> Vec<ExecCandidate> {
    let mut out = Vec::new();

    if let Ok(explicit_bin) = std::env::var("COMPUTER_USE_BIN") {
        let explicit = explicit_bin.trim();
        if !explicit.is_empty() {
            out.push(ExecCandidate {
                program: explicit.to_string(),
                prefix_args: Vec::new(),
                cwd: None,
                label: "COMPUTER_USE_BIN".to_string(),
            });
        }
    }

    if let Ok(platform_bin) = std::env::var("COMPUTER_USE_PLATFORM_BIN") {
        let explicit = platform_bin.trim();
        if !explicit.is_empty() {
            out.push(ExecCandidate {
                program: explicit.to_string(),
                prefix_args: Vec::new(),
                cwd: None,
                label: "COMPUTER_USE_PLATFORM_BIN".to_string(),
            });
        }
    }

    if let Some(docs_root) = resolve_docs_root(ctx.workspace_dir.as_path()) {
        for rel in binary_rel_paths() {
            let bin = docs_root.join(rel);
            if bin.is_file() {
                out.push(ExecCandidate {
                    program: bin.to_string_lossy().to_string(),
                    prefix_args: Vec::new(),
                    cwd: None,
                    label: "docs/computer-use binary".to_string(),
                });
            }
        }

        out.push(ExecCandidate {
            program: "cargo".to_string(),
            prefix_args: vec![
                "run".to_string(),
                "-p".to_string(),
                "computer-use".to_string(),
                "--".to_string(),
            ],
            cwd: Some(docs_root),
            label: "docs/computer-use cargo fallback".to_string(),
        });
    }

    out.push(ExecCandidate {
        program: executable_name("computer-use"),
        prefix_args: Vec::new(),
        cwd: None,
        label: "PATH computer-use".to_string(),
    });
    out.push(ExecCandidate {
        program: executable_name(platform_binary_name()),
        prefix_args: Vec::new(),
        cwd: None,
        label: "PATH platform binary".to_string(),
    });

    dedup_candidates(out)
}

fn resolve_docs_root(workspace_dir: &Path) -> Option<PathBuf> {
    if let Ok(explicit_root) = std::env::var("COMPUTER_USE_DOCS_DIR") {
        let candidate = PathBuf::from(explicit_root.trim());
        if is_computer_use_docs_root(candidate.as_path()) {
            return Some(candidate);
        }
    }

    let candidates = [
        workspace_dir.join("chat_app_server_rs/docs/computer-use"),
        workspace_dir.join("docs/computer-use"),
        workspace_dir.join("computer-use"),
    ];
    for candidate in candidates {
        if is_computer_use_docs_root(candidate.as_path()) {
            return Some(candidate);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let cwd_candidates = [
            cwd.join("chat_app_server_rs/docs/computer-use"),
            cwd.join("docs/computer-use"),
            cwd.join("computer-use"),
        ];
        for candidate in cwd_candidates {
            if is_computer_use_docs_root(candidate.as_path()) {
                return Some(candidate);
            }
        }
    }

    None
}

fn is_computer_use_docs_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file() && path.join("apps/computer-use/Cargo.toml").is_file()
}

fn binary_rel_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(PathBuf::from(format!(
        "target/release/{}",
        executable_name("computer-use")
    )));
    out.push(PathBuf::from(format!(
        "target/debug/{}",
        executable_name("computer-use")
    )));
    out.push(PathBuf::from(format!(
        "target/release/{}",
        executable_name(platform_binary_name())
    )));
    out.push(PathBuf::from(format!(
        "target/debug/{}",
        executable_name(platform_binary_name())
    )));
    out
}

fn platform_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "computer-use-windows"
    } else {
        "computer-use-macos"
    }
}

fn executable_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", base)
    } else {
        base.to_string()
    }
}

fn dedup_candidates(candidates: Vec<ExecCandidate>) -> Vec<ExecCandidate> {
    let mut out = Vec::new();
    for item in candidates {
        if out.iter().any(|existing: &ExecCandidate| {
            existing.program == item.program
                && existing.prefix_args == item.prefix_args
                && existing.cwd == item.cwd
        }) {
            continue;
        }
        out.push(item);
    }
    out
}

fn render_command_line(program: &str, prefix_args: &[String], args: &[String]) -> String {
    let mut parts = Vec::new();
    parts.push(quote_arg(program));
    for arg in prefix_args {
        parts.push(quote_arg(arg));
    }
    for arg in args {
        parts.push(quote_arg(arg));
    }
    parts.join(" ")
}

fn quote_arg(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value.contains(char::is_whitespace) || value.contains('"') || value.contains('\'') {
        let escaped = value.replace('\'', "'\"'\"'");
        return format!("'{}'", escaped);
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        is_new_tab_button_query, normalize_cli_token, normalize_command_args, parse_command_target,
        parse_json_or_text, split_command_line, ComputerUseOptions, ComputerUseService,
    };
    use serde_json::Value;

    #[test]
    fn split_command_line_supports_quotes() {
        let parsed = split_command_line(r#"click "Safari" --button "New Tab" --json"#)
            .expect("command parses");
        assert_eq!(
            parsed,
            vec!["click", "Safari", "--button", "New Tab", "--json"]
        );
    }

    #[test]
    fn split_command_line_rejects_unclosed_quote() {
        let err = split_command_line("click \"Safari").expect_err("must fail");
        assert!(err.contains("unclosed quote"));
    }

    #[test]
    fn normalize_command_args_drops_binary_prefix() {
        let parsed = vec![
            "computer-use".to_string(),
            "windows".to_string(),
            "Safari".to_string(),
            "--json".to_string(),
        ];
        let normalized = normalize_command_args(parsed);
        assert_eq!(normalized, vec!["windows", "Safari", "--json"]);
    }

    #[test]
    fn normalize_command_args_supports_cargo_run_style_input() {
        let parsed = vec![
            "cargo".to_string(),
            "run".to_string(),
            "--".to_string(),
            "click".to_string(),
            "Safari".to_string(),
            "--button".to_string(),
            "New Tab".to_string(),
            "--json".to_string(),
        ];
        let normalized = normalize_command_args(parsed);
        assert_eq!(
            normalized,
            vec!["click", "Safari", "--button", "New Tab", "--json"]
        );
    }

    #[test]
    fn computer_use_service_only_exposes_command_entry() {
        let service = ComputerUseService::new(ComputerUseOptions {
            server_name: "computer_use_test".to_string(),
            workspace_dir: ".".to_string(),
        })
        .expect("service should init");
        let tools = service.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0].get("name").and_then(Value::as_str),
            Some("command")
        );
    }

    #[test]
    fn parse_json_or_text_handles_json_and_plain_text() {
        let parsed_json = parse_json_or_text("{\"ok\":true}");
        assert_eq!(parsed_json.get("ok").and_then(Value::as_bool), Some(true));

        let parsed_text = parse_json_or_text("not-json");
        assert_eq!(parsed_text.as_str(), Some("not-json"));
    }

    #[test]
    fn normalize_cli_token_strips_smart_quotes() {
        let normalized = normalize_cli_token("“Safari”");
        assert_eq!(normalized, "Safari");
    }

    #[test]
    fn parse_command_target_supports_screenshot_with_output_path() {
        let args = vec![
            "screenshot".to_string(),
            "Safari".to_string(),
            "out.png".to_string(),
            "--json".to_string(),
        ];
        let parsed = parse_command_target(args.as_slice()).expect("target should parse");
        assert_eq!(parsed.target.as_deref(), Some("Safari"));
        assert_eq!(parsed.bundle_id, None);
    }

    #[test]
    fn is_new_tab_button_query_accepts_cn_and_en() {
        assert!(is_new_tab_button_query("New Tab"));
        assert!(is_new_tab_button_query("新标签页"));
        assert!(!is_new_tab_button_query("刷新"));
    }
}
