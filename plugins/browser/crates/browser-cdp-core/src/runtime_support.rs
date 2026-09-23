use super::*;

impl BrowserSession {
    pub(super) fn summary(&self, browser_session_id: &str) -> BrowserSessionSummary {
        BrowserSessionSummary {
            browser_session_id: browser_session_id.to_owned(),
            mode: self.mode,
            state: "open",
            active_tab_id: self.active_tab_id.clone(),
            tab_count: self.tabs.len(),
            browser: self.browser.clone(),
        }
    }

    pub(super) fn tab_session(
        &self,
        tab_id: Option<&str>,
    ) -> CoreResult<(String, BackendSessionId)> {
        let tab_id = tab_id
            .map(str::to_owned)
            .or_else(|| self.active_tab_id.clone())
            .ok_or_else(|| CoreError::NotFound("active tab".into()))?;
        let tab = self
            .tabs
            .get(&tab_id)
            .ok_or_else(|| CoreError::NotFound(format!("tab {tab_id}")))?;
        Ok((tab_id, tab.backend_session_id.clone()))
    }

    pub(super) fn invalidate_refs(&mut self) {
        self.ref_generation = self.ref_generation.wrapping_add(1);
        self.element_refs.clear();
    }
}

pub(super) async fn evaluate_value(
    backend: &dyn BrowserBackend,
    session_id: &BackendSessionId,
    expression: &str,
) -> CoreResult<Value> {
    let response = backend
        .send_command(
            Some(session_id),
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
                "userGesture": true
            }),
            COMMAND_TIMEOUT,
        )
        .await?;
    if let Some(exception) = response.get("exceptionDetails") {
        return Err(CoreError::Backend(format!(
            "page evaluation failed: {exception}"
        )));
    }
    Ok(response
        .pointer("/result/value")
        .cloned()
        .unwrap_or(Value::Null))
}

pub(super) fn virtual_cursor_move_script(selector: &str) -> String {
    format!(
        r##"(async () => {{
  const el = document.querySelector({selector});
  if (!el) throw new Error('element not found');
  el.scrollIntoView({{block:'center', inline:'center'}});
  await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) throw new Error('element is not visible');
  const x = Math.max(0, Math.min(innerWidth - 1, rect.left + rect.width / 2));
  const y = Math.max(0, Math.min(innerHeight - 1, rect.top + rect.height / 2));
  const id = {cursor_id};
  let host = document.getElementById(id);
  if (host && host.dataset.chatosVirtualMouse !== 'true') {{
    host.remove();
    host = null;
  }}
  if (!host) {{
    host = document.createElement('div');
    host.id = id;
    host.dataset.chatosVirtualMouse = 'true';
    host.setAttribute('aria-hidden', 'true');
    Object.assign(host.style, {{
      position: 'fixed', left: '0', top: '0', width: '1px', height: '1px',
      pointerEvents: 'none', zIndex: '2147483647', opacity: '1',
      willChange: 'transform', transition: 'transform {move_ms}ms cubic-bezier(.2,.8,.2,1)'
    }});
    const root = host.attachShadow ? host.attachShadow({{mode:'open'}}) : host;
    root.innerHTML = {markup};
    (document.documentElement || document.body).appendChild(host);
    const startX = Math.max(0, innerWidth / 2);
    const startY = Math.max(0, innerHeight / 2);
    host.style.transition = 'none';
    host.style.transform = `translate3d(${{startX}}px, ${{startY}}px, 0)`;
    host.getBoundingClientRect();
    host.style.transition = 'transform {move_ms}ms cubic-bezier(.2,.8,.2,1)';
  }}
  host.style.opacity = '1';
  host.style.transform = `translate3d(${{x}}px, ${{y}}px, 0)`;
  host.dataset.x = String(x);
  host.dataset.y = String(y);
  await new Promise(resolve => setTimeout(resolve, {move_wait_ms}));
  return {{x, y}};
}})()"##,
        selector = selector,
        cursor_id = serde_json::to_string(VIRTUAL_CURSOR_ID).unwrap(),
        markup = serde_json::to_string(VIRTUAL_CURSOR_MARKUP).unwrap(),
        move_ms = VIRTUAL_CURSOR_MOVE_MS,
        move_wait_ms = VIRTUAL_CURSOR_MOVE_MS + 30,
    )
}

pub(super) fn virtual_cursor_restore_script(x: f64, y: f64) -> String {
    format!(
        r##"(() => {{
  const mount = () => {{
    const id = {cursor_id};
    let host = document.getElementById(id);
    if (host && host.dataset.chatosVirtualMouse !== 'true') {{
      host.remove();
      host = null;
    }}
    if (!host) {{
      host = document.createElement('div');
      host.id = id;
      host.dataset.chatosVirtualMouse = 'true';
      host.setAttribute('aria-hidden', 'true');
      Object.assign(host.style, {{
        position:'fixed', left:'0', top:'0', width:'1px', height:'1px',
        pointerEvents:'none', zIndex:'2147483647', opacity:'1',
        willChange:'transform', transition:'none'
      }});
      const root = host.attachShadow ? host.attachShadow({{mode:'open'}}) : host;
      root.innerHTML = {markup};
      (document.documentElement || document.body).appendChild(host);
    }}
    const x = Math.max(0, Math.min(innerWidth - 1, {x}));
    const y = Math.max(0, Math.min(innerHeight - 1, {y}));
    host.style.transform = `translate3d(${{x}}px, ${{y}}px, 0)`;
    host.dataset.x = String(x);
    host.dataset.y = String(y);
  }};
  if (document.documentElement) mount();
  else addEventListener('DOMContentLoaded', mount, {{once:true}});
}})()"##,
        cursor_id = serde_json::to_string(VIRTUAL_CURSOR_ID).unwrap(),
        markup = serde_json::to_string(VIRTUAL_CURSOR_MARKUP).unwrap(),
        x = x,
        y = y,
    )
}

pub(super) fn virtual_cursor_pulse_script() -> String {
    format!(
        r#"(() => {{
  const host = document.getElementById({cursor_id});
  if (!host || !host.shadowRoot) return false;
  const pulse = host.shadowRoot.querySelector('.pulse');
  const pointer = host.shadowRoot.querySelector('.pointer');
  if (!pulse || !pointer) return false;
  pulse.classList.remove('active');
  pointer.classList.remove('pressed');
  void pulse.offsetWidth;
  pulse.classList.add('active');
  pointer.classList.add('pressed');
  setTimeout(() => pointer.classList.remove('pressed'), 180);
  return true;
}})()"#,
        cursor_id = serde_json::to_string(VIRTUAL_CURSOR_ID).unwrap()
    )
}

#[cfg(test)]
mod virtual_cursor_tests {
    use super::*;

    #[test]
    fn cursor_script_targets_the_element_and_draws_an_overlay() {
        let script = virtual_cursor_move_script("\"#submit\"");
        assert!(script.contains("document.querySelector(\"#submit\")"));
        assert!(script.contains(VIRTUAL_CURSOR_ID));
        assert!(script.contains("attachShadow"));
        assert!(script.contains("transform 280ms"));
        assert!(script.contains("animation:chatos-click 620ms"));
        assert!(script.contains("linearGradient"));
        assert!(script.contains("return {x, y}"));
    }

    #[test]
    fn cursor_restore_script_keeps_the_last_position_across_navigation() {
        let script = virtual_cursor_restore_script(320.5, 180.25);
        assert!(script.contains(VIRTUAL_CURSOR_ID));
        assert!(script.contains("DOMContentLoaded"));
        assert!(script.contains("320.5"));
        assert!(script.contains("180.25"));
        assert!(script.contains("transition:'none'"));
    }

    #[test]
    fn cursor_pulse_uses_the_owned_shadow_root() {
        let script = virtual_cursor_pulse_script();
        assert!(script.contains(VIRTUAL_CURSOR_ID));
        assert!(script.contains("querySelector('.pulse')"));
    }
}

pub(super) async fn read_title(
    backend: &dyn BrowserBackend,
    session_id: &BackendSessionId,
) -> CoreResult<String> {
    evaluate_value(backend, session_id, "document.title")
        .await?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Backend("document title is not a string".into()))
}

pub(super) async fn wait_until_ready(
    backend: &dyn BrowserBackend,
    session_id: &BackendSessionId,
    timeout: Duration,
) -> CoreResult<()> {
    let deadline = Instant::now()
        + timeout
            .min(Duration::from_secs(60))
            .max(Duration::from_millis(100));
    loop {
        match evaluate_value(backend, session_id, "document.readyState === 'complete'").await {
            Ok(Value::Bool(true)) => return Ok(()),
            Ok(_) | Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Ok(_) | Err(_) => return Err(CoreError::Timeout("navigation".into())),
        }
    }
}

pub(super) fn opaque_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

pub(super) fn ensure_within(root: &Path, path: &Path) -> CoreResult<()> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(CoreError::InvalidRequest(
            "artifact path escaped artifact directory".into(),
        ))
    }
}

pub(super) fn sanitize_artifact_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(160)
        .collect::<String>();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "download.bin".into()
    } else {
        sanitized
    }
}

pub(super) fn mime_type_for_name(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => "application/json",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

pub(super) fn validate_route_rule(rule: &RouteRule) -> CoreResult<()> {
    if rule.url_pattern.is_empty() || rule.url_pattern.len() > 4_096 {
        return Err(CoreError::InvalidRequest(
            "url_pattern must contain between 1 and 4096 characters".into(),
        ));
    }
    if let RouteAction::MockJson { status, body } = &rule.action {
        if !(100..=599).contains(status) {
            return Err(CoreError::InvalidRequest(
                "mock_json status must be between 100 and 599".into(),
            ));
        }
        if serde_json::to_vec(body).map_or(true, |bytes| bytes.len() > 512 * 1024) {
            return Err(CoreError::InvalidRequest(
                "mock_json body exceeds 512 KiB".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Default)]
struct HarEntryState {
    request_id: String,
    started_date_time: Option<String>,
    request_timestamp: Option<f64>,
    response_timestamp: Option<f64>,
    end_timestamp: Option<f64>,
    method: String,
    url: String,
    request_headers: Value,
    post_data: Option<String>,
    status: i64,
    status_text: String,
    response_headers: Value,
    mime_type: String,
    protocol: String,
    encoded_data_length: i64,
    error_text: Option<String>,
}

pub(super) fn build_har(batch: EventBatch) -> Value {
    let mut entries: HashMap<String, HarEntryState> = HashMap::new();
    for event in batch.events {
        let Some(request_id) = event.params.get("requestId").and_then(Value::as_str) else {
            continue;
        };
        let entry = entries
            .entry(request_id.to_owned())
            .or_insert_with(|| HarEntryState {
                request_id: request_id.to_owned(),
                ..Default::default()
            });
        match event.method.as_str() {
            "Network.requestWillBeSent" => {
                let request = event.params.get("request").unwrap_or(&Value::Null);
                entry.method = request
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET")
                    .to_owned();
                entry.url = request
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                entry.request_headers =
                    request.get("headers").cloned().unwrap_or_else(|| json!({}));
                entry.post_data = request
                    .get("postData")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                entry.request_timestamp = event.params.get("timestamp").and_then(Value::as_f64);
                entry.started_date_time = event
                    .params
                    .get("wallTime")
                    .and_then(Value::as_f64)
                    .and_then(|seconds| {
                        chrono::DateTime::<Utc>::from_timestamp_millis((seconds * 1_000.0) as i64)
                    })
                    .map(|time| time.to_rfc3339());
            }
            "Network.requestWillBeSentExtraInfo" => {
                if let Some(headers) = event.params.get("headers") {
                    merge_header_objects(&mut entry.request_headers, headers);
                }
            }
            "Network.responseReceived" => {
                let response = event.params.get("response").unwrap_or(&Value::Null);
                entry.status = response
                    .get("status")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as i64;
                entry.status_text = response
                    .get("statusText")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                entry.response_headers = response
                    .get("headers")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                entry.mime_type = response
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                entry.protocol = response
                    .get("protocol")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                entry.response_timestamp = event.params.get("timestamp").and_then(Value::as_f64);
            }
            "Network.responseReceivedExtraInfo" => {
                if let Some(headers) = event.params.get("headers") {
                    merge_header_objects(&mut entry.response_headers, headers);
                }
                if entry.status == 0 {
                    entry.status = event
                        .params
                        .get("statusCode")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                }
            }
            "Network.loadingFinished" => {
                entry.end_timestamp = event.params.get("timestamp").and_then(Value::as_f64);
                entry.encoded_data_length = event
                    .params
                    .get("encodedDataLength")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as i64;
            }
            "Network.loadingFailed" => {
                entry.end_timestamp = event.params.get("timestamp").and_then(Value::as_f64);
                entry.error_text = event
                    .params
                    .get("errorText")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            _ => {}
        }
    }

    let mut entries = entries.into_values().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.request_timestamp
            .partial_cmp(&right.request_timestamp)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let entries = entries
        .into_iter()
        .map(|entry| {
            let total_time = duration_ms_between(entry.request_timestamp, entry.end_timestamp);
            let wait_time = duration_ms_between(entry.request_timestamp, entry.response_timestamp);
            let receive_time = duration_ms_between(entry.response_timestamp, entry.end_timestamp);
            json!({
                "_requestId": entry.request_id,
                "startedDateTime": entry.started_date_time.unwrap_or_else(|| Utc::now().to_rfc3339()),
                "time": total_time,
                "request": {
                    "method": if entry.method.is_empty() { "GET" } else { &entry.method },
                    "url": entry.url,
                    "httpVersion": entry.protocol,
                    "cookies": [],
                    "headers": headers_to_har(&entry.request_headers),
                    "queryString": [],
                    "headersSize": -1,
                    "bodySize": entry.post_data.as_ref().map_or(0, |body| body.len() as i64),
                    "postData": entry.post_data.map(|text| json!({ "mimeType": "", "text": text }))
                },
                "response": {
                    "status": entry.status,
                    "statusText": entry.error_text.unwrap_or(entry.status_text),
                    "httpVersion": entry.protocol,
                    "cookies": [],
                    "headers": headers_to_har(&entry.response_headers),
                    "content": {
                        "size": entry.encoded_data_length,
                        "mimeType": entry.mime_type
                    },
                    "redirectURL": "",
                    "headersSize": -1,
                    "bodySize": entry.encoded_data_length
                },
                "cache": {},
                "timings": {
                    "blocked": -1,
                    "dns": -1,
                    "connect": -1,
                    "send": 0,
                    "wait": wait_time,
                    "receive": receive_time,
                    "ssl": -1
                }
            })
        })
        .collect::<Vec<_>>();

    json!({
        "log": {
            "version": "1.2",
            "creator": { "name": "chatos-browser-cdp", "version": env!("CARGO_PKG_VERSION") },
            "pages": [],
            "entries": entries,
            "_droppedEventCount": batch.dropped_event_count
        }
    })
}

fn merge_header_objects(target: &mut Value, source: &Value) {
    let target = target.as_object_mut();
    let source = source.as_object();
    if let (Some(target), Some(source)) = (target, source) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn headers_to_har(headers: &Value) -> Vec<Value> {
    headers
        .as_object()
        .map(|headers| {
            headers
                .iter()
                .map(|(name, value)| {
                    json!({
                        "name": name,
                        "value": value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn duration_ms_between(start: Option<f64>, end: Option<f64>) -> f64 {
    match (start, end) {
        (Some(start), Some(end)) if end >= start => (end - start) * 1_000.0,
        _ => 0.0,
    }
}

#[allow(dead_code)]
fn _target_to_tab(target: TargetDescriptor) -> TabSummary {
    TabSummary {
        tab_id: target.id,
        title: target.title,
        url: target.url,
        active: false,
    }
}

#[allow(dead_code)]
const _DEFAULT_NAVIGATION_TIMEOUT: Duration = NAVIGATION_TIMEOUT;
