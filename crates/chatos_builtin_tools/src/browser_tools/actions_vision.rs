use serde_json::{json, Value};
use uuid::Uuid;

use super::super::{context, BoundContext, BrowserVisionRequest};
use super::actions_shared::{fail_json, is_success, normalize_inline_text, run_browser_command};

pub(super) async fn browser_vision_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    question: String,
    annotate: bool,
) -> Result<Value, String> {
    let Some(adapter) = ctx.vision_adapter.clone() else {
        let question = normalize_inline_text(question.as_str(), 220);
        return Ok(json!({
            "_summary_text": "Browser vision is unavailable in the portable builtin tools runtime.",
            "success": false,
            "question": question,
            "vision": {
                "enabled": false,
                "mode": "unavailable",
                "error": "browser_vision requires a host-provided vision model adapter. Use browser_inspect, browser_snapshot, or browser_research in the portable runtime."
            }
        }));
    };

    let session = context::conversation_key(conversation_id);
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

    let request = BrowserVisionRequest {
        question: question.clone(),
        screenshot_path: actual_path.clone(),
        conversation_id: conversation_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        annotate,
    };
    let (analysis, vision) = match adapter.analyze_screenshot(request).await {
        Ok(output) => (output.analysis, output.vision),
        Err(err) => (
            "Screenshot captured, but vision analysis was unavailable. See vision.error and vision.attempts.".to_string(),
            json!({
                "enabled": false,
                "mode": "unavailable",
                "error": err.error,
                "attempts": err.attempts,
                "warnings": err.warnings,
            }),
        ),
    };

    Ok(json!({
        "_summary_text": format!(
            "Captured a browser screenshot and produced vision analysis (vision available: {}, mode: {}, transport: {}).",
            if vision.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                "yes"
            } else {
                "no"
            },
            vision.get("mode").and_then(|v| v.as_str()).unwrap_or("unknown"),
            vision
                .get("transport")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        ),
        "success": true,
        "analysis": analysis,
        "question": question,
        "screenshot_path": actual_path,
        "annotations": result.get("data").and_then(|v| v.get("annotations")).cloned().unwrap_or(Value::Null),
        "vision": vision,
    }))
}
