use std::env;
use std::process::Stdio;

use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine as _;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::chat_runtime::{compose_contact_system_prompt, ChatRuntimeMetadata};
use crate::repositories::ai_model_configs;
use crate::services::memory_server_client;
use crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v3::message_manager::MessageManager;

use super::{BoundContext, BrowserSession};

const SCROLL_PIXELS: i32 = 500;
const DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS: i64 = 700;

pub(super) async fn browser_navigate_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    url: String,
) -> Result<Value, String> {
    let session = super::context::session_key(session_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "open",
        vec![url.clone()],
        ctx.command_timeout_seconds.max(60),
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Navigation failed"));
    }

    let data = result.get("data").cloned().unwrap_or_else(|| json!({}));
    let final_url = data
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or(url.as_str())
        .to_string();
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut response = json!({
        "success": true,
        "url": final_url,
        "title": title
    });

    let snap_result = run_browser_command(
        &ctx,
        session.as_str(),
        "snapshot",
        vec!["-c".to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if is_success(&snap_result) {
        let snap_data = snap_result
            .get("data")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let snapshot = snap_data
            .get("snapshot")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let refs = snap_data.get("refs").and_then(|v| v.as_object());
        response["snapshot"] = Value::String(truncate_chars(snapshot, ctx.max_snapshot_chars));
        response["element_count"] = json!(refs.map(|v| v.len()).unwrap_or(0));
    }

    Ok(response)
}

pub(super) async fn browser_snapshot_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    full: bool,
) -> Result<Value, String> {
    let session = super::context::session_key(session_id);
    let args = if full { vec![] } else { vec!["-c".to_string()] };
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "snapshot",
        args,
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to get snapshot"));
    }

    let data = result.get("data").cloned().unwrap_or_else(|| json!({}));
    let snapshot = data.get("snapshot").and_then(|v| v.as_str()).unwrap_or("");
    let refs = data.get("refs").and_then(|v| v.as_object());

    Ok(json!({
        "success": true,
        "snapshot": truncate_chars(snapshot, ctx.max_snapshot_chars),
        "element_count": refs.map(|v| v.len()).unwrap_or(0),
    }))
}

pub(super) async fn browser_click_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    mut reference: String,
) -> Result<Value, String> {
    if !reference.starts_with('@') {
        reference = format!("@{}", reference.trim());
    }
    let session = super::context::session_key(session_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "click",
        vec![reference.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to click {}", reference).as_str(),
        ));
    }

    Ok(json!({
        "success": true,
        "clicked": reference
    }))
}

pub(super) async fn browser_type_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    mut reference: String,
    text: String,
) -> Result<Value, String> {
    if !reference.starts_with('@') {
        reference = format!("@{}", reference.trim());
    }
    let session = super::context::session_key(session_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "fill",
        vec![reference.clone(), text.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to type into {}", reference).as_str(),
        ));
    }

    Ok(json!({
        "success": true,
        "typed": text,
        "element": reference
    }))
}

pub(super) async fn browser_scroll_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    direction: String,
) -> Result<Value, String> {
    if direction != "up" && direction != "down" {
        return Ok(json!({
            "success": false,
            "error": format!("Invalid direction '{}'. Use 'up' or 'down'.", direction)
        }));
    }
    let session = super::context::session_key(session_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "scroll",
        vec![direction.clone(), SCROLL_PIXELS.to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to scroll {}", direction).as_str(),
        ));
    }

    Ok(json!({
        "success": true,
        "scrolled": direction
    }))
}

pub(super) async fn browser_back_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::context::session_key(session_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "back",
        vec![],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to go back"));
    }

    let url = result
        .get("data")
        .and_then(|data| data.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(json!({
        "success": true,
        "url": url,
    }))
}

pub(super) async fn browser_press_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    key: String,
) -> Result<Value, String> {
    let session = super::context::session_key(session_id);
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "press",
        vec![key.clone()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(
            &result,
            format!("Failed to press {}", key).as_str(),
        ));
    }
    Ok(json!({
        "success": true,
        "pressed": key
    }))
}

pub(super) async fn browser_console_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    clear: bool,
    expression: Option<String>,
) -> Result<Value, String> {
    let session = super::context::session_key(session_id);
    if let Some(expression) = expression {
        let result = run_browser_command(
            &ctx,
            session.as_str(),
            "eval",
            vec![expression],
            ctx.command_timeout_seconds,
        )
        .await?;
        if !is_success(&result) {
            return Ok(fail_json(&result, "eval failed"));
        }

        let raw = result
            .get("data")
            .and_then(|v| v.get("result"))
            .cloned()
            .unwrap_or(Value::Null);
        let parsed = if let Some(text) = raw.as_str() {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
        } else {
            raw
        };
        return Ok(json!({
            "success": true,
            "result": parsed,
            "result_type": result_type_name(&parsed),
        }));
    }

    let mut console_args = Vec::new();
    if clear {
        console_args.push("--clear".to_string());
    }
    let console_result = run_browser_command(
        &ctx,
        session.as_str(),
        "console",
        console_args.clone(),
        ctx.command_timeout_seconds,
    )
    .await?;
    let errors_result = run_browser_command(
        &ctx,
        session.as_str(),
        "errors",
        console_args,
        ctx.command_timeout_seconds,
    )
    .await?;

    let mut messages: Vec<Value> = Vec::new();
    if is_success(&console_result) {
        if let Some(arr) = console_result
            .get("data")
            .and_then(|v| v.get("messages"))
            .and_then(|v| v.as_array())
        {
            for item in arr {
                let typ = item.get("type").and_then(|v| v.as_str()).unwrap_or("log");
                let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
                messages.push(json!({
                    "type": typ,
                    "text": text,
                    "source": "console"
                }));
            }
        }
    }

    let mut errors: Vec<Value> = Vec::new();
    if is_success(&errors_result) {
        if let Some(arr) = errors_result
            .get("data")
            .and_then(|v| v.get("errors"))
            .and_then(|v| v.as_array())
        {
            for item in arr {
                let text = item.get("message").and_then(|v| v.as_str()).unwrap_or("");
                errors.push(json!({
                    "message": text,
                    "source": "exception"
                }));
            }
        }
    }

    Ok(json!({
        "success": true,
        "console_messages": messages,
        "js_errors": errors,
        "total_messages": messages.len(),
        "total_errors": errors.len(),
    }))
}

pub(super) async fn browser_get_images_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::context::session_key(session_id);
    let js = r#"JSON.stringify(
        [...document.images].map(img => ({
            src: img.src,
            alt: img.alt || '',
            width: img.naturalWidth,
            height: img.naturalHeight
        })).filter(img => img.src && !img.src.startsWith('data:'))
    )"#;
    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "eval",
        vec![js.to_string()],
        ctx.command_timeout_seconds,
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to get images"));
    }
    let raw = result
        .get("data")
        .and_then(|v| v.get("result"))
        .cloned()
        .unwrap_or_else(|| Value::String("[]".to_string()));
    let parsed = if let Some(text) = raw.as_str() {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!([]))
    } else {
        raw
    };
    let count = parsed.as_array().map(|v| v.len()).unwrap_or(0);
    Ok(json!({
        "success": true,
        "images": parsed,
        "count": count
    }))
}

pub(super) async fn browser_vision_with_context(
    ctx: BoundContext,
    session_id: Option<&str>,
    question: String,
    annotate: bool,
) -> Result<Value, String> {
    let session = super::context::session_key(session_id);
    let screenshot_dir = ctx
        .workspace_dir
        .join(".chatos")
        .join("browser_screenshots");
    std::fs::create_dir_all(&screenshot_dir)
        .map_err(|err| format!("create screenshot dir failed: {}", err))?;
    let screenshot_path = screenshot_dir.join(format!(
        "browser_screenshot_{}.png",
        Uuid::new_v4().simple()
    ));
    let mut args = Vec::new();
    if annotate {
        args.push("--annotate".to_string());
    }
    args.push("--full".to_string());
    args.push(screenshot_path.to_string_lossy().to_string());

    let result = run_browser_command(
        &ctx,
        session.as_str(),
        "screenshot",
        args,
        ctx.command_timeout_seconds.max(60),
    )
    .await?;
    if !is_success(&result) {
        return Ok(fail_json(&result, "Failed to take screenshot"));
    }

    let actual_path = result
        .get("data")
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| screenshot_path.to_string_lossy().to_string());

    let (analysis, vision) = match analyze_screenshot_with_current_contact(
        question.as_str(),
        actual_path.as_str(),
        session_id,
    )
    .await
    {
        Ok(output) => (
            output.analysis,
            json!({
                "enabled": true,
                "mode": "contact_agent",
                "contact_agent_id": output.contact_agent_id,
                "model": output.model,
                "provider": output.provider,
            }),
        ),
        Err(err) => (
            "Screenshot captured, but contact agent analysis failed. See vision.error."
                .to_string(),
            json!({
                "enabled": false,
                "mode": "contact_agent",
                "error": err
            }),
        ),
    };

    Ok(json!({
        "success": true,
        "analysis": analysis,
        "question": question,
        "screenshot_path": actual_path,
        "annotations": result.get("data").and_then(|v| v.get("annotations")).cloned().unwrap_or(Value::Null),
        "vision": vision,
    }))
}

#[derive(Debug, Clone)]
struct ContactVisionOutput {
    analysis: String,
    contact_agent_id: String,
    model: String,
    provider: String,
}

async fn analyze_screenshot_with_current_contact(
    question: &str,
    screenshot_path: &str,
    session_id: Option<&str>,
) -> Result<ContactVisionOutput, String> {
    let session_id = normalize_non_empty(session_id).ok_or_else(|| {
        "browser_vision requires an active session_id to resolve current contact".to_string()
    })?;
    let session = memory_server_client::get_session_by_id(session_id.as_str())
        .await?
        .ok_or_else(|| format!("session not found: {}", session_id))?;
    let metadata_runtime = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    let contact_agent_id = normalize_non_empty(session.selected_agent_id.as_deref())
        .or_else(|| metadata_runtime.contact_agent_id.clone())
        .ok_or_else(|| "current session has no selected contact agent".to_string())?;
    let contact_runtime =
        memory_server_client::get_memory_agent_runtime_context(contact_agent_id.as_str())
            .await?
            .ok_or_else(|| {
                format!(
                    "contact runtime context not found for agent {}",
                    contact_agent_id
                )
            })?;
    let contact_system_prompt =
        compose_contact_system_prompt(Some(&contact_runtime)).unwrap_or_default();
    let model_cfg_value = load_session_model_cfg_value(&session).await?;
    let cfg = Config::get();
    let model_runtime = resolve_chat_model_config(
        &model_cfg_value,
        "gpt-4o",
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        Some(true),
        true,
    );
    if model_runtime.api_key.trim().is_empty() {
        return Err("current session model has no api key configured".to_string());
    }

    let image_bytes = tokio::fs::read(screenshot_path)
        .await
        .map_err(|err| format!("read screenshot failed: {}", err))?;
    let mime = mime_guess::from_path(screenshot_path).first_or_octet_stream();
    let image_data_url = format!(
        "data:{};base64,{}",
        mime.essence_str(),
        BASE64_STD.encode(image_bytes)
    );

    let prompt = format!(
        "你现在收到了一张当前网页截图。请仅基于截图内容回答用户问题，先给结论，再给1-3条关键依据。用户问题：{}",
        question
    );
    let input = json!([
        {
            "type": "message",
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": prompt
                },
                {
                    "type": "input_image",
                    "image_url": image_data_url
                }
            ]
        }
    ]);
    let instructions = normalize_non_empty(Some(contact_system_prompt.as_str()));
    let handler = AiRequestHandler::new(
        model_runtime.api_key.clone(),
        model_runtime.base_url.clone(),
        MessageManager::new(),
    );
    let response = handler
        .handle_request(
            input,
            model_runtime.model.clone(),
            instructions,
            None,
            None,
            None,
            Some(model_runtime.temperature),
            Some(DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS),
            StreamCallbacks::default(),
            Some(model_runtime.provider.clone()),
            model_runtime.thinking_level.clone(),
            None,
            None,
            false,
            None,
            None,
            "browser_vision_contact",
        )
        .await
        .map_err(|err| format!("contact vision request failed: {}", err))?;
    let analysis = if !response.content.trim().is_empty() {
        response.content.trim().to_string()
    } else {
        response.reasoning.unwrap_or_default().trim().to_string()
    };
    if analysis.trim().is_empty() {
        return Err("contact vision response did not include text output".to_string());
    }

    Ok(ContactVisionOutput {
        analysis,
        contact_agent_id,
        model: model_runtime.model,
        provider: model_runtime.provider,
    })
}

async fn load_session_model_cfg_value(
    session: &crate::models::session::Session,
) -> Result<Value, String> {
    let Some(model_id) = normalize_non_empty(session.selected_model_id.as_deref()) else {
        return Ok(json!({}));
    };
    let Some(model_cfg) = ai_model_configs::get_ai_model_config_by_id(model_id.as_str()).await?
    else {
        return Ok(json!({}));
    };
    serde_json::to_value(model_cfg).map_err(|err| format!("serialize model config failed: {}", err))
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

async fn run_browser_command(
    ctx: &BoundContext,
    session_key: &str,
    command: &str,
    args: Vec<String>,
    timeout_seconds: u64,
) -> Result<Value, String> {
    let session = get_or_create_session(ctx, session_key);
    let (program, prefix) = resolve_agent_browser_cmd()?;
    let mut cmd = Command::new(program);
    cmd.current_dir(&ctx.workspace_dir);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for value in prefix {
        cmd.arg(value);
    }

    if let Some(cdp_url) = session.cdp_url {
        cmd.arg("--cdp").arg(cdp_url);
    } else {
        cmd.arg("--session").arg(session.session_name);
    }
    cmd.arg("--json").arg(command);
    for value in args {
        cmd.arg(value);
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("spawn browser command failed: {}", err))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "missing browser stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "missing browser stderr".to_string())?;
    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.map(|_| buf)
    });
    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.map(|_| buf)
    });

    let status = match timeout(Duration::from_secs(timeout_seconds.max(1)), child.wait()).await {
        Ok(result) => result.map_err(|err| format!("wait browser command failed: {}", err))?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(json!({
                "success": false,
                "error": format!("Command timed out after {} seconds", timeout_seconds.max(1))
            }));
        }
    };

    let stdout = stdout_task
        .await
        .map_err(|err| format!("read browser stdout join failed: {}", err))?
        .map_err(|err| format!("read browser stdout failed: {}", err))?;
    let stderr = stderr_task
        .await
        .map_err(|err| format!("read browser stderr join failed: {}", err))?
        .map_err(|err| format!("read browser stderr failed: {}", err))?;

    let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
    if stdout_text.is_empty() && status.success() && command != "close" && command != "record" {
        return Ok(json!({
            "success": false,
            "error": format!("Browser command '{}' returned no output", command)
        }));
    }

    if !stdout_text.is_empty() {
        match serde_json::from_str::<Value>(&stdout_text) {
            Ok(parsed) => return Ok(parsed),
            Err(err) => {
                return Ok(json!({
                    "success": false,
                    "error": format!(
                        "Non-JSON output from agent-browser for '{}': {}",
                        command,
                        truncate_chars(&stdout_text, 2000)
                    ),
                    "detail": err.to_string(),
                }));
            }
        }
    }

    if !status.success() {
        return Ok(json!({
            "success": false,
            "error": if stderr_text.is_empty() {
                format!("Browser command failed with status {}", status)
            } else {
                stderr_text
            }
        }));
    }

    Ok(json!({
        "success": true,
        "data": {}
    }))
}

fn get_or_create_session(ctx: &BoundContext, session_key: &str) -> BrowserSession {
    let mut sessions = ctx.sessions.lock();
    if let Some(existing) = sessions.get(session_key) {
        return existing.clone();
    }

    let cdp_override = env::var("BROWSER_CDP_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let session = BrowserSession {
        session_name: if cdp_override.is_some() {
            format!("cdp_{}", Uuid::new_v4().simple())
        } else {
            format!("h_{}", Uuid::new_v4().simple())
        },
        cdp_url: cdp_override,
    };
    sessions.insert(session_key.to_string(), session.clone());
    session
}

fn resolve_agent_browser_cmd() -> Result<(String, Vec<String>), String> {
    if let Some(value) = env::var("AGENT_BROWSER_BIN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return Ok((value, vec![]));
    }
    if command_exists("agent-browser") {
        return Ok(("agent-browser".to_string(), vec![]));
    }
    if command_exists("npx") {
        return Ok(("npx".to_string(), vec!["agent-browser".to_string()]));
    }
    Err(
        "agent-browser CLI not found. Install with: npm install -g agent-browser && agent-browser install"
            .to_string(),
    )
}

fn command_exists(program: &str) -> bool {
    let path_value = match env::var_os("PATH") {
        Some(value) => value,
        None => return false,
    };
    for dir in env::split_paths(&path_value) {
        let full = dir.join(program);
        if full.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            let full_exe = dir.join(format!("{}.exe", program));
            if full_exe.is_file() {
                return true;
            }
        }
    }
    false
}

fn is_success(value: &Value) -> bool {
    value
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn fail_json(value: &Value, fallback: &str) -> Value {
    let error = value
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string();
    json!({
        "success": false,
        "error": error
    })
}

fn result_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}
