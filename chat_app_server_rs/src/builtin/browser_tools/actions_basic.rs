use serde_json::{json, Value};

use super::actions_shared::{
    apply_snapshot_payload, browser_result_data, build_browser_action_summary,
    enrich_response_with_page_metadata, fail_json, finalize_browser_action_response, is_success,
    normalize_ref, parse_browser_eval_payload, run_basic_browser_action, run_browser_command,
};
use super::BoundContext;

const SCROLL_PIXELS: i32 = 500;

pub(super) async fn browser_navigate_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    url: String,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
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

    let data = browser_result_data(&result);
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

    let response = json!({
        "success": true,
        "url": final_url,
        "title": title
    });
    Ok(finalize_browser_action_response(
        &ctx,
        session.as_str(),
        response,
        "Opened page.",
        Some("Use refs from the snapshot with browser_click or browser_type."),
    )
    .await)
}

pub(super) async fn browser_snapshot_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    full: bool,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
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

    let data = browser_result_data(&result);
    let mut response = json!({
        "success": true,
    });
    apply_snapshot_payload(&mut response, &data, ctx.max_snapshot_chars);
    enrich_response_with_page_metadata(&ctx, session.as_str(), &mut response).await;
    response["_summary_text"] = Value::String(build_browser_action_summary(
        "Captured page snapshot.",
        &response,
        Some("Use refs like @e12 from the snapshot when clicking or typing."),
    ));
    Ok(response)
}

pub(super) async fn browser_click_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    mut reference: String,
) -> Result<Value, String> {
    reference = normalize_ref(reference);
    let session = super::super::context::conversation_key(conversation_id);
    run_basic_browser_action(
        &ctx,
        session.as_str(),
        "click",
        vec![reference.clone()],
        ctx.command_timeout_seconds,
        format!("Failed to click {}", reference),
        json!({
            "success": true,
            "clicked": reference
        }),
        "Clicked element.",
        None,
    )
    .await
}

pub(super) async fn browser_type_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    mut reference: String,
    text: String,
) -> Result<Value, String> {
    reference = normalize_ref(reference);
    let session = super::super::context::conversation_key(conversation_id);
    run_basic_browser_action(
        &ctx,
        session.as_str(),
        "fill",
        vec![reference.clone(), text.clone()],
        ctx.command_timeout_seconds,
        format!("Failed to type into {}", reference),
        json!({
            "success": true,
            "typed": text,
            "typed_chars": text.chars().count(),
            "element": reference
        }),
        "Typed into element.",
        None,
    )
    .await
}

pub(super) async fn browser_scroll_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    direction: String,
) -> Result<Value, String> {
    if direction != "up" && direction != "down" {
        return Ok(json!({
            "_summary_text": format!("Browser scroll failed because direction '{}' is invalid.", direction),
            "success": false,
            "error": format!("Invalid direction '{}'. Use 'up' or 'down'.", direction)
        }));
    }
    let session = super::super::context::conversation_key(conversation_id);
    run_basic_browser_action(
        &ctx,
        session.as_str(),
        "scroll",
        vec![direction.clone(), SCROLL_PIXELS.to_string()],
        ctx.command_timeout_seconds,
        format!("Failed to scroll {}", direction),
        json!({
            "success": true,
            "scrolled": direction
        }),
        "Scrolled page.",
        None,
    )
    .await
}

pub(super) async fn browser_back_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
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

    let data = browser_result_data(&result);
    let url = data
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let response = json!({
        "success": true,
        "url": url,
    });
    Ok(finalize_browser_action_response(
        &ctx,
        session.as_str(),
        response,
        "Navigated back in history.",
        None,
    )
    .await)
}

pub(super) async fn browser_press_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
    key: String,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
    run_basic_browser_action(
        &ctx,
        session.as_str(),
        "press",
        vec![key.clone()],
        ctx.command_timeout_seconds,
        format!("Failed to press {}", key),
        json!({
            "success": true,
            "pressed": key
        }),
        "Pressed keyboard key.",
        None,
    )
    .await
}

pub(super) async fn browser_get_images_with_context(
    ctx: BoundContext,
    conversation_id: Option<&str>,
) -> Result<Value, String> {
    let session = super::super::context::conversation_key(conversation_id);
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
    let parsed = parse_browser_eval_payload(raw);
    let images = parsed.as_array().cloned().unwrap_or_default();
    let count = images.len();
    Ok(json!({
        "_summary_text": format!("Found {} image(s) in the current page DOM.", count),
        "success": true,
        "images": images,
        "count": count
    }))
}
