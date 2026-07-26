// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};
use tokio_tungstenite::WebSocketStream;
use url::{Host, Url};

use crate::browser_command_support::{
    browser_command_succeeded, parse_browser_command_eval_payload,
};
use crate::browser_runtime::{
    run_browser_command as runtime_run_browser_command, BrowserRuntimeSession,
};

use super::{
    normalize_browser_session_conversation_id, BrowserSessionPreviewFrame, BrowserToolsService,
};

pub(super) const CDP_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
pub(super) const CDP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const CDP_MAX_MESSAGE_BYTES: usize = 12 * 1024 * 1024;
const PREVIEW_PDF_MAX_BYTES: usize = 8 * 1024 * 1024;
const PREVIEW_MAX_DIMENSION: u32 = 4_096;
const PREVIEW_MAX_PIXELS: u64 = 16 * 1024 * 1024;
const CSS_PIXELS_PER_INCH: f64 = 96.0;

#[derive(Debug, Clone, Copy)]
struct PageMetrics {
    width: u32,
    height: u32,
    scroll_x: f64,
    scroll_y: f64,
    scroll_height: f64,
}

impl PageMetrics {
    fn first_page(self) -> u32 {
        ((self.scroll_y / f64::from(self.height)).floor() as u32).saturating_add(1)
    }

    fn page_count(self) -> u32 {
        ((self.scroll_height / f64::from(self.height)).ceil() as u32).max(1)
    }

    fn last_page(self) -> u32 {
        self.first_page().saturating_add(1).min(self.page_count())
    }

    fn crop_offset_y(self) -> f64 {
        self.scroll_y % f64::from(self.height)
    }
}

impl BrowserToolsService {
    pub async fn capture_attached_managed_session_preview_frame(
        &self,
        conversation_id: &str,
    ) -> Result<BrowserSessionPreviewFrame, String> {
        self.capture_attached_managed_session_preview_frame_after(conversation_id, 0)
            .await?
            .ok_or_else(|| "managed browser preview did not produce a frame".to_string())
    }

    pub async fn capture_attached_managed_session_preview_frame_after(
        &self,
        conversation_id: &str,
        after_sequence: u64,
    ) -> Result<Option<BrowserSessionPreviewFrame>, String> {
        let conversation_key = normalize_browser_session_conversation_id(conversation_id)?;
        let session = self
            .bound
            .sessions
            .lock()
            .get(conversation_key.as_str())
            .cloned()
            .ok_or_else(|| "managed browser session is not attached".to_string())?;
        if session.cdp_url.is_some() {
            return Err(
                "CDP browser sessions cannot be captured by the managed session UI".to_string(),
            );
        }

        match super::managed_screencast::capture_browser_screencast_frame(
            &self.bound,
            conversation_key.as_str(),
            after_sequence,
        )
        .await
        {
            Ok(frame) => Ok(frame),
            Err(screencast_error) => {
                let (metrics, page_url) = self.managed_session_page_metrics(&session).await?;
                let endpoint = self.managed_session_cdp_endpoint(&session).await?;
                let bytes =
                    capture_bounded_pdf_preview(endpoint.as_str(), metrics, page_url.as_str())
                        .await?;
                let timestamp = current_timestamp_ms();
                Ok(Some(BrowserSessionPreviewFrame {
                    bytes,
                    media_type: "application/pdf",
                    sequence: timestamp.max(after_sequence.saturating_add(1)),
                    width: metrics.width,
                    height: metrics.height,
                    page_scale_factor: 1.0,
                    offset_top: 0.0,
                    scroll_offset_x: metrics.scroll_x,
                    scroll_offset_y: metrics.scroll_y,
                    crop_offset_y: metrics.crop_offset_y(),
                    timestamp,
                    source: "cdp_pdf_preview",
                    warning: Some(format!(
                        "continuous CDP screencast is unavailable; using bounded PDF preview: {screencast_error}"
                    )),
                }))
            }
        }
    }

    pub fn stop_attached_managed_session_preview_stream(
        &self,
        conversation_id: &str,
    ) -> Result<bool, String> {
        let conversation_key = normalize_browser_session_conversation_id(conversation_id)?;
        Ok(super::managed_screencast::stop_browser_screencast(
            &self.bound,
            conversation_key.as_str(),
        ))
    }

    async fn managed_session_page_metrics(
        &self,
        session: &BrowserRuntimeSession,
    ) -> Result<(PageMetrics, String), String> {
        let expression = r#"JSON.stringify({width:window.innerWidth,height:window.innerHeight,scrollX:window.scrollX,scrollY:window.scrollY,scrollHeight:Math.max(document.documentElement?.scrollHeight||0,document.body?.scrollHeight||0,window.innerHeight)})"#;
        let response = runtime_run_browser_command(
            self.bound.workspace_dir.as_path(),
            session,
            "eval",
            vec![expression.to_string()],
            3,
        )
        .await?;
        if !browser_command_succeeded(&response) {
            return Err("browser preview page metrics are unavailable".to_string());
        }
        let raw = response
            .pointer("/data/result")
            .cloned()
            .ok_or_else(|| "browser preview page metrics are malformed".to_string())?;
        let page_url = response
            .pointer("/data/origin")
            .and_then(Value::as_str)
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 8_192
                    && !value.chars().any(|character| character.is_control())
            })
            .ok_or_else(|| "browser preview page identity is malformed".to_string())?;
        Ok((
            parse_page_metrics(&parse_browser_command_eval_payload(raw))?,
            page_url.to_string(),
        ))
    }

    async fn managed_session_cdp_endpoint(
        &self,
        session: &BrowserRuntimeSession,
    ) -> Result<Url, String> {
        let response = runtime_run_browser_command(
            self.bound.workspace_dir.as_path(),
            session,
            "get",
            vec!["cdp-url".to_string()],
            3,
        )
        .await?;
        if !browser_command_succeeded(&response) {
            return Err("managed browser CDP endpoint is unavailable".to_string());
        }
        let value = response
            .pointer("/data/cdpUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| "managed browser CDP endpoint is malformed".to_string())?;
        validate_loopback_cdp_endpoint(value)
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn parse_page_metrics(value: &Value) -> Result<PageMetrics, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "browser preview page metrics are malformed".to_string())?;
    let width = bounded_dimension(object.get("width"), "width")?;
    let height = bounded_dimension(object.get("height"), "height")?;
    if u64::from(width).saturating_mul(u64::from(height)) > PREVIEW_MAX_PIXELS {
        return Err("browser preview viewport exceeds the pixel limit".to_string());
    }
    let scroll_x = bounded_nonnegative_number(object.get("scrollX"), "scrollX")?;
    let scroll_y = bounded_nonnegative_number(object.get("scrollY"), "scrollY")?;
    let scroll_height = bounded_nonnegative_number(object.get("scrollHeight"), "scrollHeight")?
        .max(f64::from(height));
    let maximum_reasonable_scroll =
        (scroll_height - f64::from(height)).max(0.0) + f64::from(height);
    if scroll_y > maximum_reasonable_scroll {
        return Err("browser preview scroll offset exceeds the document bounds".to_string());
    }
    Ok(PageMetrics {
        width,
        height,
        scroll_x,
        scroll_y,
        scroll_height,
    })
}

fn bounded_dimension(value: Option<&Value>, field: &str) -> Result<u32, String> {
    let number = value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && number.fract() == 0.0)
        .filter(|number| *number >= 1.0 && *number <= f64::from(PREVIEW_MAX_DIMENSION))
        .ok_or_else(|| format!("browser preview page metrics contain invalid {field}"))?;
    Ok(number as u32)
}

fn bounded_nonnegative_number(value: Option<&Value>, field: &str) -> Result<f64, String> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number >= 0.0 && *number <= 1_000_000_000.0)
        .ok_or_else(|| format!("browser preview page metrics contain invalid {field}"))
}

pub(super) fn validate_loopback_cdp_endpoint(value: &str) -> Result<Url, String> {
    let endpoint =
        Url::parse(value).map_err(|_| "managed browser CDP endpoint is malformed".to_string())?;
    if endpoint.scheme() != "ws"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || endpoint.port().is_none()
        || !endpoint.path().starts_with("/devtools/browser/")
    {
        return Err("managed browser CDP endpoint is not an approved loopback URL".to_string());
    }
    let is_loopback = match endpoint.host() {
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    if !is_loopback {
        return Err("managed browser CDP endpoint is not loopback-only".to_string());
    }
    Ok(endpoint)
}

async fn capture_bounded_pdf_preview(
    endpoint: &str,
    metrics: PageMetrics,
    page_url: &str,
) -> Result<Vec<u8>, String> {
    let config = WebSocketConfig::default()
        .read_buffer_size(32 * 1024)
        .write_buffer_size(8 * 1024)
        .max_write_buffer_size(64 * 1024)
        .max_message_size(Some(CDP_MAX_MESSAGE_BYTES))
        .max_frame_size(Some(CDP_MAX_MESSAGE_BYTES));
    let (mut stream, _) = tokio::time::timeout(
        CDP_CONNECT_TIMEOUT,
        connect_async_with_config(endpoint, Some(config), true),
    )
    .await
    .map_err(|_| "managed browser CDP connection timed out".to_string())?
    .map_err(|_| "managed browser CDP connection failed".to_string())?;

    tokio::time::timeout(CDP_RESPONSE_TIMEOUT, async {
        send_cdp_command(&mut stream, 1, "Target.getTargets", json!({}), None).await?;
        let targets = read_cdp_result(&mut stream, 1).await?;
        let target_id = active_page_target_id(&targets, page_url)?;

        send_cdp_command(
            &mut stream,
            2,
            "Target.attachToTarget",
            json!({"targetId": target_id, "flatten": true}),
            None,
        )
        .await?;
        let attached = read_cdp_result(&mut stream, 2).await?;
        let session_id = attached
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 256)
            .ok_or_else(|| "managed browser CDP attachment is malformed".to_string())?;

        let first_page = metrics.first_page();
        let last_page = metrics.last_page();
        let page_ranges = if first_page == last_page {
            first_page.to_string()
        } else {
            format!("{first_page}-{last_page}")
        };
        send_cdp_command(
            &mut stream,
            3,
            "Page.printToPDF",
            json!({
                "printBackground": true,
                "landscape": false,
                "displayHeaderFooter": false,
                "preferCSSPageSize": false,
                "paperWidth": f64::from(metrics.width) / CSS_PIXELS_PER_INCH,
                "paperHeight": f64::from(metrics.height) / CSS_PIXELS_PER_INCH,
                "marginTop": 0,
                "marginBottom": 0,
                "marginLeft": 0,
                "marginRight": 0,
                "pageRanges": page_ranges,
                "scale": 1,
                "generateTaggedPDF": false,
                "generateDocumentOutline": false,
            }),
            Some(session_id),
        )
        .await?;
        let printed = read_cdp_result(&mut stream, 3).await?;
        decode_bounded_pdf(&printed)
    })
    .await
    .map_err(|_| "managed browser PDF preview timed out".to_string())?
}

pub(super) async fn send_cdp_command<S>(
    stream: &mut WebSocketStream<S>,
    id: u64,
    method: &str,
    params: Value,
    session_id: Option<&str>,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut command = json!({"id": id, "method": method, "params": params});
    if let Some(session_id) = session_id {
        command["sessionId"] = Value::String(session_id.to_string());
    }
    stream
        .send(Message::Text(command.to_string().into()))
        .await
        .map_err(|_| "managed browser CDP command failed".to_string())
}

pub(super) async fn read_cdp_result<S>(
    stream: &mut WebSocketStream<S>,
    expected_id: u64,
) -> Result<Value, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut messages_seen = 0_usize;
    while let Some(message) = stream.next().await {
        messages_seen = messages_seen.saturating_add(1);
        if messages_seen > 256 {
            return Err("managed browser CDP emitted too many messages".to_string());
        }
        let message = message.map_err(|_| "managed browser CDP read failed".to_string())?;
        if !message.is_text() {
            continue;
        }
        let text = message
            .to_text()
            .map_err(|_| "managed browser CDP emitted invalid text".to_string())?;
        if text.len() > CDP_MAX_MESSAGE_BYTES {
            return Err("managed browser CDP response exceeded the message limit".to_string());
        }
        let response: Value = serde_json::from_str(text)
            .map_err(|_| "managed browser CDP emitted malformed JSON".to_string())?;
        if response.get("id").and_then(Value::as_u64) != Some(expected_id) {
            continue;
        }
        if response.get("error").is_some() {
            return Err("managed browser CDP rejected the preview command".to_string());
        }
        return response
            .get("result")
            .cloned()
            .ok_or_else(|| "managed browser CDP response is missing its result".to_string());
    }
    Err("managed browser CDP closed before returning a result".to_string())
}

pub(super) fn active_page_target_id(result: &Value, expected_url: &str) -> Result<String, String> {
    let candidates = result
        .get("targetInfos")
        .and_then(Value::as_array)
        .map(|targets| {
            targets
                .iter()
                .filter_map(|target| {
                    let target_type = target.get("type").and_then(Value::as_str)?;
                    let attached = target.get("attached").and_then(Value::as_bool)?;
                    let url = target
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let target_id = target.get("targetId").and_then(Value::as_str)?;
                    if target_type == "page"
                        && attached
                        && !is_internal_browser_url(url)
                        && !target_id.is_empty()
                        && target_id.len() <= 256
                    {
                        Some((target_id.to_string(), url.to_string()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut exact_matches = candidates.iter().filter(|(_, url)| url == expected_url);
    if let Some((target_id, _)) = exact_matches.next() {
        if exact_matches.next().is_none() {
            return Ok(target_id.clone());
        }
        return Err("managed browser active page target is ambiguous".to_string());
    }
    if candidates.len() == 1 {
        return Ok(candidates[0].0.clone());
    }
    Err("managed browser active page target is unavailable".to_string())
}

fn is_internal_browser_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("chrome://")
        || lower.starts_with("chrome-extension://")
        || lower.starts_with("devtools://")
}

fn decode_bounded_pdf(result: &Value) -> Result<Vec<u8>, String> {
    let encoded = result
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| "managed browser PDF preview is missing data".to_string())?;
    if encoded.len() > CDP_MAX_MESSAGE_BYTES {
        return Err("managed browser PDF preview exceeded the encoded limit".to_string());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "managed browser PDF preview contains invalid base64".to_string())?;
    if bytes.is_empty() || bytes.len() > PREVIEW_PDF_MAX_BYTES {
        return Err("managed browser PDF preview exceeded the decoded limit".to_string());
    }
    if !bytes.starts_with(b"%PDF-")
        || !bytes[bytes.len().saturating_sub(1_024)..]
            .windows(5)
            .any(|window| window == b"%%EOF")
    {
        return Err("managed browser PDF preview is malformed".to_string());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use serde_json::json;

    use super::{
        active_page_target_id, decode_bounded_pdf, parse_page_metrics,
        validate_loopback_cdp_endpoint,
    };

    #[test]
    fn accepts_only_private_browser_cdp_endpoints() {
        let endpoint = validate_loopback_cdp_endpoint(
            "ws://127.0.0.1:49152/devtools/browser/01234567-89ab-cdef",
        )
        .expect("loopback CDP endpoint");
        assert_eq!(endpoint.port(), Some(49152));
        assert!(validate_loopback_cdp_endpoint(
            "ws://example.com:49152/devtools/browser/01234567-89ab-cdef"
        )
        .is_err());
        assert!(
            validate_loopback_cdp_endpoint("ws://127.0.0.1:49152/devtools/page/secret").is_err()
        );
    }

    #[test]
    fn parses_and_bounds_page_metrics() {
        let metrics = parse_page_metrics(&json!({
            "width": 1280,
            "height": 720,
            "scrollX": 0,
            "scrollY": 900,
            "scrollHeight": 2400
        }))
        .expect("page metrics");
        assert_eq!(metrics.first_page(), 2);
        assert_eq!(metrics.last_page(), 3);
        assert_eq!(metrics.crop_offset_y(), 180.0);
        assert!(parse_page_metrics(&json!({
            "width": 8192,
            "height": 8192,
            "scrollX": 0,
            "scrollY": 0,
            "scrollHeight": 8192
        }))
        .is_err());
    }

    #[test]
    fn selects_attached_non_internal_page_target() {
        let target = active_page_target_id(&json!({
            "targetInfos": [
                {"type": "page", "attached": true, "url": "https://other.example/", "targetId": "other"},
                {"type": "page", "attached": true, "url": "https://example.com/", "targetId": "active"}
            ]
        }), "https://example.com/")
        .expect("active target");
        assert_eq!(target, "active");
    }

    #[test]
    fn rejects_duplicate_attached_pages_with_the_same_current_url() {
        let error = active_page_target_id(
            &json!({
                "targetInfos": [
                    {"type": "page", "attached": true, "url": "https://example.com/", "targetId": "first"},
                    {"type": "page", "attached": true, "url": "https://example.com/", "targetId": "second"}
                ]
            }),
            "https://example.com/",
        )
        .expect_err("duplicate matching targets must fail closed");
        assert!(error.contains("ambiguous"));
    }

    #[test]
    fn validates_bounded_pdf_payload() {
        let bytes = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n%%EOF\n";
        let decoded =
            decode_bounded_pdf(&json!({"data": STANDARD.encode(bytes)})).expect("bounded PDF");
        assert_eq!(decoded, bytes);
        assert!(decode_bounded_pdf(&json!({"data": STANDARD.encode(b"not a pdf")})).is_err());
    }
}
