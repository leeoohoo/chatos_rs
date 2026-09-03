use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use browser_cdp_policy::{validate_cdp_command, validate_navigation_url};
use browser_cdp_protocol::{
    ArtifactDescriptor, BackendSessionId, BrowserDescriptor, BrowserMode, EventBatch, EventFilter,
    OpenBrowserRequest, RouteAction, RouteDescriptor, RouteRule, TargetDescriptor,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{BrowserBackend, BrowserBackendFactory};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(15);
const VIRTUAL_CURSOR_ID: &str = "__chatos_virtual_mouse__";
const SNAPSHOT_SCRIPT: &str = r#"(() => {
  const selectorFor = (el) => {
    if (el.id && CSS.escape) return `#${CSS.escape(el.id)}`;
    const parts = [];
    let node = el;
    while (node && node.nodeType === Node.ELEMENT_NODE && node !== document.body) {
      let part = node.tagName.toLowerCase();
      const siblings = node.parentElement ? [...node.parentElement.children].filter(x => x.tagName === node.tagName) : [];
      if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(node) + 1})`;
      parts.unshift(part);
      node = node.parentElement;
    }
    return `body > ${parts.join(' > ')}`;
  };
  const roleFor = (el) => el.getAttribute('role') || ({A:'link',BUTTON:'button',INPUT:'textbox',TEXTAREA:'textbox',SELECT:'combobox',IMG:'img',H1:'heading',H2:'heading',H3:'heading'}[el.tagName] || 'generic');
  return [...document.querySelectorAll('a,button,input,textarea,select,[role],[contenteditable="true"],h1,h2,h3')]
    .filter(el => {
      const style = getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    })
    .slice(0, 500)
    .map(el => ({
      role: roleFor(el),
      name: el.getAttribute('aria-label') || el.getAttribute('alt') || el.getAttribute('title') || (el.innerText || el.value || '').trim().slice(0, 300),
      value: typeof el.value === 'string' ? el.value.slice(0, 300) : null,
      tag: el.tagName.toLowerCase(),
      selector: selectorFor(el),
      disabled: !!el.disabled
    }));
})()"#;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("browser backend error: {0}")]
    Backend(String),
    #[error("operation timed out: {0}")]
    Timeout(String),
    #[error("I/O error: {0}")]
    Io(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, Serialize)]
pub struct BrowserSessionSummary {
    pub browser_session_id: String,
    pub mode: BrowserMode,
    pub state: &'static str,
    pub active_tab_id: Option<String>,
    pub tab_count: usize,
    pub browser: BrowserDescriptor,
}

#[derive(Debug, Clone, Serialize)]
pub struct TabSummary {
    pub tab_id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotNode {
    pub reference: String,
    pub role: String,
    pub name: String,
    pub value: Option<String>,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadCollection {
    pub events: EventBatch,
    pub artifacts: Vec<ArtifactDescriptor>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSnapshotNode {
    role: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: Option<String>,
    selector: String,
    #[serde(default)]
    disabled: bool,
}

#[derive(Debug)]
struct TabState {
    public_id: String,
    backend_target_id: String,
    backend_session_id: BackendSessionId,
    title: Option<String>,
    url: Option<String>,
}

#[derive(Debug)]
struct ElementReference {
    tab_id: String,
    generation: u64,
    selector: String,
}

struct BrowserSession {
    mode: BrowserMode,
    browser: BrowserDescriptor,
    backend: Arc<dyn BrowserBackend>,
    tabs: HashMap<String, TabState>,
    active_tab_id: Option<String>,
    cdp_sessions: HashMap<String, BackendSessionId>,
    subscriptions: HashMap<String, String>,
    routes: HashMap<String, RouteState>,
    downloads: HashMap<String, DownloadState>,
    used_file_grants: HashSet<String>,
    element_refs: HashMap<String, ElementReference>,
    ref_generation: u64,
}

#[derive(Debug, Deserialize)]
struct FileGrantDescriptor {
    path: PathBuf,
    expires_at_unix_ms: u64,
    size: u64,
    sha256: String,
}

#[derive(Default)]
struct DownloadState {
    suggested_filename: Option<String>,
    artifact: Option<ArtifactDescriptor>,
}

struct RouteState {
    backend_route_id: String,
    descriptor: RouteDescriptor,
}

pub struct BrowserRuntime {
    factories: Vec<Arc<dyn BrowserBackendFactory>>,
    sessions: RwLock<HashMap<String, Arc<Mutex<BrowserSession>>>>,
    artifact_dir: PathBuf,
}

impl BrowserRuntime {
    pub fn new(
        factories: Vec<Arc<dyn BrowserBackendFactory>>,
        artifact_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            factories,
            sessions: RwLock::new(HashMap::new()),
            artifact_dir: artifact_dir.into(),
        }
    }

    pub async fn open_session(
        &self,
        request: OpenBrowserRequest,
    ) -> CoreResult<BrowserSessionSummary> {
        let factory = self
            .factories
            .iter()
            .find(|factory| factory.supports(request.mode))
            .ok_or_else(|| CoreError::Unsupported(format!("browser mode {:?}", request.mode)))?;
        let backend = factory.create(request.mode).await?;
        let browser = backend.open(request.clone()).await?;
        let mut targets = backend.list_targets().await?;
        if request.mode == BrowserMode::ChromeExtension
            && browser
                .capabilities
                .iter()
                .any(|capability| capability == "native_tab_groups")
        {
            targets.insert(0, backend.create_target("about:blank").await?);
        } else if targets.is_empty() {
            targets.push(backend.create_target("about:blank").await?);
        }

        let mut tabs = HashMap::new();
        let mut active_tab_id = None;
        for target in targets.into_iter().filter(|target| target.kind == "page") {
            let tab_id = opaque_id("tab");
            let backend_session_id = backend.attach_target(&target.id).await?;
            active_tab_id.get_or_insert_with(|| tab_id.clone());
            tabs.insert(
                tab_id.clone(),
                TabState {
                    public_id: tab_id,
                    backend_target_id: target.id,
                    backend_session_id,
                    title: target.title,
                    url: target.url,
                },
            );
        }
        let browser_session_id = opaque_id("bs");
        let session = BrowserSession {
            mode: request.mode,
            browser: browser.clone(),
            backend,
            tabs,
            active_tab_id,
            cdp_sessions: HashMap::new(),
            subscriptions: HashMap::new(),
            routes: HashMap::new(),
            downloads: HashMap::new(),
            used_file_grants: HashSet::new(),
            element_refs: HashMap::new(),
            ref_generation: 0,
        };
        let summary = session.summary(&browser_session_id);
        self.sessions
            .write()
            .await
            .insert(browser_session_id, Arc::new(Mutex::new(session)));
        Ok(summary)
    }

    pub async fn session_status(
        &self,
        browser_session_id: &str,
    ) -> CoreResult<BrowserSessionSummary> {
        let session = self.session(browser_session_id).await?;
        Ok(session.lock().await.summary(browser_session_id))
    }

    pub async fn close_session(&self, browser_session_id: &str) -> CoreResult<()> {
        let session = self
            .sessions
            .write()
            .await
            .remove(browser_session_id)
            .ok_or_else(|| CoreError::NotFound(format!("browser session {browser_session_id}")))?;
        let backend = session.lock().await.backend.clone();
        backend.close().await
    }

    pub async fn close_all(&self) {
        let sessions = {
            let mut sessions = self.sessions.write().await;
            sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in sessions {
            let backend = session.lock().await.backend.clone();
            let _ = backend.close().await;
        }
    }

    pub async fn tabs(&self, browser_session_id: &str) -> CoreResult<Vec<TabSummary>> {
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let mut tabs = session
            .tabs
            .values()
            .map(|tab| TabSummary {
                tab_id: tab.public_id.clone(),
                title: tab.title.clone(),
                url: tab.url.clone(),
                active: session.active_tab_id.as_deref() == Some(tab.public_id.as_str()),
            })
            .collect::<Vec<_>>();
        tabs.sort_by(|left, right| left.tab_id.cmp(&right.tab_id));
        Ok(tabs)
    }

    pub async fn new_tab(&self, browser_session_id: &str, url: &str) -> CoreResult<TabSummary> {
        validate_navigation_url(url)
            .map_err(|error| CoreError::InvalidRequest(error.to_string()))?;
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let target = session.backend.create_target(url).await?;
        let backend_session_id = session.backend.attach_target(&target.id).await?;
        let tab_id = opaque_id("tab");
        let tab = TabState {
            public_id: tab_id.clone(),
            backend_target_id: target.id,
            backend_session_id,
            title: target.title,
            url: target.url,
        };
        session.active_tab_id = Some(tab_id.clone());
        let summary = TabSummary {
            tab_id: tab_id.clone(),
            title: tab.title.clone(),
            url: tab.url.clone(),
            active: true,
        };
        session.tabs.insert(tab_id, tab);
        session.invalidate_refs();
        Ok(summary)
    }

    pub async fn switch_tab(
        &self,
        browser_session_id: &str,
        tab_id: &str,
    ) -> CoreResult<TabSummary> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let tab = session
            .tabs
            .get(tab_id)
            .ok_or_else(|| CoreError::NotFound(format!("tab {tab_id}")))?;
        let summary = TabSummary {
            tab_id: tab.public_id.clone(),
            title: tab.title.clone(),
            url: tab.url.clone(),
            active: true,
        };
        session.active_tab_id = Some(tab_id.to_owned());
        Ok(summary)
    }

    pub async fn close_tab(&self, browser_session_id: &str, tab_id: &str) -> CoreResult<()> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let tab = session
            .tabs
            .remove(tab_id)
            .ok_or_else(|| CoreError::NotFound(format!("tab {tab_id}")))?;
        let route_ids = session
            .routes
            .iter()
            .filter(|(_, route)| route.descriptor.tab_id == tab_id)
            .map(|(route_id, _)| route_id.clone())
            .collect::<Vec<_>>();
        for route_id in route_ids {
            if let Some(route) = session.routes.remove(&route_id) {
                session
                    .backend
                    .remove_route(&route.backend_route_id)
                    .await?;
            }
        }
        session.backend.close_target(&tab.backend_target_id).await?;
        if session.active_tab_id.as_deref() == Some(tab_id) {
            session.active_tab_id = session.tabs.keys().next().cloned();
        }
        session.invalidate_refs();
        Ok(())
    }

    pub async fn navigate(
        &self,
        browser_session_id: &str,
        tab_id: Option<&str>,
        url: &str,
        timeout: Duration,
    ) -> CoreResult<Value> {
        validate_navigation_url(url)
            .map_err(|error| CoreError::InvalidRequest(error.to_string()))?;
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let (tab_id, backend_session_id) = session.tab_session(tab_id)?;
        let result = session
            .backend
            .send_command(
                Some(&backend_session_id),
                "Page.navigate",
                json!({ "url": url }),
                timeout.min(Duration::from_secs(60)),
            )
            .await?;
        wait_until_ready(session.backend.as_ref(), &backend_session_id, timeout).await?;
        let title = read_title(session.backend.as_ref(), &backend_session_id)
            .await
            .ok();
        if let Some(tab) = session.tabs.get_mut(&tab_id) {
            tab.url = Some(url.to_owned());
            tab.title = title;
        }
        session.invalidate_refs();
        Ok(result)
    }

    pub async fn snapshot(
        &self,
        browser_session_id: &str,
        tab_id: Option<&str>,
    ) -> CoreResult<Vec<SnapshotNode>> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let (tab_id, backend_session_id) = session.tab_session(tab_id)?;
        let value = evaluate_value(
            session.backend.as_ref(),
            &backend_session_id,
            SNAPSHOT_SCRIPT,
        )
        .await?;
        let nodes: Vec<RawSnapshotNode> = serde_json::from_value(value)
            .map_err(|error| CoreError::Backend(format!("invalid snapshot response: {error}")))?;
        session.invalidate_refs();
        let generation = session.ref_generation;
        let mut snapshots = Vec::with_capacity(nodes.len());
        for node in nodes {
            let reference = opaque_id("ref");
            session.element_refs.insert(
                reference.clone(),
                ElementReference {
                    tab_id: tab_id.clone(),
                    generation,
                    selector: node.selector,
                },
            );
            snapshots.push(SnapshotNode {
                reference,
                role: node.role,
                name: node.name,
                value: node.value,
                disabled: node.disabled,
            });
        }
        Ok(snapshots)
    }

    pub async fn find(
        &self,
        browser_session_id: &str,
        query: &str,
        max_results: usize,
    ) -> CoreResult<Vec<SnapshotNode>> {
        let snapshot = self.snapshot(browser_session_id, None).await?;
        let query = query.to_lowercase();
        Ok(snapshot
            .into_iter()
            .filter(|node| {
                node.name.to_lowercase().contains(&query)
                    || node.role.to_lowercase().contains(&query)
                    || node
                        .value
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            })
            .take(max_results.clamp(1, 100))
            .collect())
    }

    pub async fn click(&self, browser_session_id: &str, reference: &str) -> CoreResult<Value> {
        let (backend, backend_session_id, selector) =
            self.resolve_ref(browser_session_id, reference).await?;
        let selector = serde_json::to_string(&selector).unwrap();
        let point = evaluate_value(
            backend.as_ref(),
            &backend_session_id,
            &virtual_cursor_move_script(&selector),
        )
        .await?;
        let x = point.get("x").and_then(Value::as_f64).ok_or_else(|| {
            CoreError::Backend("click target did not return an x coordinate".into())
        })?;
        let y = point.get("y").and_then(Value::as_f64).ok_or_else(|| {
            CoreError::Backend("click target did not return a y coordinate".into())
        })?;

        backend
            .send_command(
                Some(&backend_session_id),
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mouseMoved",
                    "x": x,
                    "y": y,
                    "button": "none",
                    "buttons": 0,
                    "pointerType": "mouse"
                }),
                COMMAND_TIMEOUT,
            )
            .await?;
        backend
            .send_command(
                Some(&backend_session_id),
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mousePressed",
                    "x": x,
                    "y": y,
                    "button": "left",
                    "buttons": 1,
                    "clickCount": 1,
                    "pointerType": "mouse"
                }),
                COMMAND_TIMEOUT,
            )
            .await?;
        let _ = evaluate_value(
            backend.as_ref(),
            &backend_session_id,
            &virtual_cursor_pulse_script(),
        )
        .await;
        backend
            .send_command(
                Some(&backend_session_id),
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mouseReleased",
                    "x": x,
                    "y": y,
                    "button": "left",
                    "buttons": 0,
                    "clickCount": 1,
                    "pointerType": "mouse"
                }),
                COMMAND_TIMEOUT,
            )
            .await?;
        Ok(Value::Bool(true))
    }

    pub async fn type_text(
        &self,
        browser_session_id: &str,
        reference: &str,
        text: &str,
        clear: bool,
    ) -> CoreResult<Value> {
        let (backend, backend_session_id, selector) =
            self.resolve_ref(browser_session_id, reference).await?;
        let selector = serde_json::to_string(&selector).unwrap();
        let text = serde_json::to_string(text).unwrap();
        evaluate_value(
            backend.as_ref(),
            &backend_session_id,
            &format!("(() => {{ const el = document.querySelector({selector}); if (!el) throw new Error('element not found'); el.focus(); const next = {} ? {text} : String(el.value || '') + {text}; const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value')?.set; if (setter) setter.call(el, next); else el.value = next; el.dispatchEvent(new Event('input', {{bubbles:true}})); el.dispatchEvent(new Event('change', {{bubbles:true}})); return next; }})()", if clear { "true" } else { "false" }),
        )
        .await
    }

    pub async fn press(&self, browser_session_id: &str, key: &str) -> CoreResult<Value> {
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let (_, backend_session_id) = session.tab_session(None)?;
        let key_json = serde_json::to_string(key).unwrap();
        evaluate_value(
            session.backend.as_ref(),
            &backend_session_id,
            &format!("(() => {{ const el = document.activeElement || document.body; for (const type of ['keydown','keyup']) el.dispatchEvent(new KeyboardEvent(type, {{key:{key_json}, bubbles:true}})); if ({key_json} === 'Enter' && el.form) el.form.requestSubmit(); return true; }})()"),
        ).await
    }

    pub async fn scroll(
        &self,
        browser_session_id: &str,
        delta_x: i64,
        delta_y: i64,
    ) -> CoreResult<Value> {
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let (_, backend_session_id) = session.tab_session(None)?;
        evaluate_value(
            session.backend.as_ref(),
            &backend_session_id,
            &format!("(() => {{ window.scrollBy({delta_x}, {delta_y}); return {{x:scrollX,y:scrollY}}; }})()"),
        ).await
    }

    pub async fn wait(
        &self,
        browser_session_id: &str,
        selector: Option<&str>,
        text: Option<&str>,
        timeout: Duration,
    ) -> CoreResult<Value> {
        let session = self.session(browser_session_id).await?;
        let (backend, backend_session_id) = {
            let session = session.lock().await;
            let (_, backend_session_id) = session.tab_session(None)?;
            (session.backend.clone(), backend_session_id)
        };
        let deadline = Instant::now() + timeout.min(Duration::from_secs(20));
        loop {
            let expression = match (selector, text) {
                (Some(selector), _) => format!(
                    "!!document.querySelector({})",
                    serde_json::to_string(selector).unwrap()
                ),
                (_, Some(text)) => format!(
                    "(document.body?.innerText || '').includes({})",
                    serde_json::to_string(text).unwrap()
                ),
                _ => "document.readyState === 'complete'".to_owned(),
            };
            if evaluate_value(backend.as_ref(), &backend_session_id, &expression).await?
                == Value::Bool(true)
            {
                return Ok(json!({ "matched": true }));
            }
            if Instant::now() >= deadline {
                return Err(CoreError::Timeout("browser_wait".into()));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn screenshot(
        &self,
        browser_session_id: &str,
        full_page: bool,
    ) -> CoreResult<ArtifactDescriptor> {
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let (_, backend_session_id) = session.tab_session(None)?;
        let params = if full_page {
            json!({ "format": "png", "captureBeyondViewport": true })
        } else {
            json!({ "format": "png", "captureBeyondViewport": false })
        };
        let result = session
            .backend
            .send_command(
                Some(&backend_session_id),
                "Page.captureScreenshot",
                params,
                Duration::from_secs(10),
            )
            .await?;
        let encoded = result
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Backend("screenshot response did not include data".into()))?;
        let bytes = BASE64
            .decode(encoded)
            .map_err(|error| CoreError::Backend(format!("invalid screenshot data: {error}")))?;
        self.write_artifact("screenshot.png", "image/png", &bytes)
            .await
    }

    pub async fn handle_dialog(
        &self,
        browser_session_id: &str,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> CoreResult<Value> {
        if prompt_text.is_some_and(|text| text.len() > 16_384) {
            return Err(CoreError::InvalidRequest(
                "prompt_text exceeds 16384 characters".into(),
            ));
        }
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let (_, backend_session_id) = session.tab_session(None)?;
        let mut params = json!({ "accept": accept });
        if let Some(prompt_text) = prompt_text {
            params["promptText"] = Value::String(prompt_text.to_owned());
        }
        session
            .backend
            .send_command(
                Some(&backend_session_id),
                "Page.handleJavaScriptDialog",
                params,
                COMMAND_TIMEOUT,
            )
            .await
    }

    pub async fn upload(
        &self,
        browser_session_id: &str,
        reference: &str,
        file_grant_ids: &[String],
    ) -> CoreResult<Value> {
        if file_grant_ids.is_empty() || file_grant_ids.len() > 20 {
            return Err(CoreError::InvalidRequest(
                "file_grant_ids must contain between 1 and 20 grants".into(),
            ));
        }
        let unique = file_grant_ids.iter().collect::<HashSet<_>>();
        if unique.len() != file_grant_ids.len() {
            return Err(CoreError::InvalidRequest(
                "file_grant_ids must not contain duplicates".into(),
            ));
        }
        let session = self.session(browser_session_id).await?;
        let (backend, backend_session_id, selector) = {
            let session = session.lock().await;
            for file_grant_id in file_grant_ids {
                if session.used_file_grants.contains(file_grant_id) {
                    return Err(CoreError::InvalidRequest(format!(
                        "file grant {file_grant_id} has already been consumed"
                    )));
                }
            }
            let element = session.element_refs.get(reference).ok_or_else(|| {
                CoreError::NotFound(format!(
                    "element reference {reference}; take a new snapshot"
                ))
            })?;
            if element.generation != session.ref_generation {
                return Err(CoreError::InvalidRequest(
                    "element reference is stale; take a new snapshot".into(),
                ));
            }
            let tab = session
                .tabs
                .get(&element.tab_id)
                .ok_or_else(|| CoreError::NotFound(format!("tab {}", element.tab_id)))?;
            (
                session.backend.clone(),
                tab.backend_session_id.clone(),
                element.selector.clone(),
            )
        };

        let mut files = Vec::with_capacity(file_grant_ids.len());
        for file_grant_id in file_grant_ids {
            files.push(self.resolve_file_grant(file_grant_id).await?);
        }
        let selector_json = serde_json::to_string(&selector).unwrap();
        let is_file_input = evaluate_value(
            backend.as_ref(),
            &backend_session_id,
            &format!(
                "(() => {{ const el = document.querySelector({selector_json}); return !!el && el.tagName === 'INPUT' && el.type === 'file'; }})()"
            ),
        )
        .await?;
        if is_file_input != Value::Bool(true) {
            return Err(CoreError::InvalidRequest(
                "element reference does not identify an input[type=file]".into(),
            ));
        }
        let document = backend
            .send_command(
                Some(&backend_session_id),
                "DOM.getDocument",
                json!({ "depth": 0, "pierce": true }),
                COMMAND_TIMEOUT,
            )
            .await?;
        let node_id = document
            .pointer("/root/nodeId")
            .and_then(Value::as_i64)
            .ok_or_else(|| CoreError::Backend("DOM.getDocument returned no root node".into()))?;
        let selected = backend
            .send_command(
                Some(&backend_session_id),
                "DOM.querySelector",
                json!({ "nodeId": node_id, "selector": selector }),
                COMMAND_TIMEOUT,
            )
            .await?;
        let input_node_id = selected
            .get("nodeId")
            .and_then(Value::as_i64)
            .filter(|node_id| *node_id != 0)
            .ok_or_else(|| CoreError::NotFound("file input DOM node".into()))?;
        backend
            .send_command(
                Some(&backend_session_id),
                "DOM.setFileInputFiles",
                json!({
                    "files": files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                    "nodeId": input_node_id
                }),
                Duration::from_secs(10),
            )
            .await?;
        session
            .lock()
            .await
            .used_file_grants
            .extend(file_grant_ids.iter().cloned());
        Ok(json!({
            "uploaded_file_count": files.len(),
            "consumed_file_grant_ids": file_grant_ids
        }))
    }

    pub async fn downloads_start(&self, browser_session_id: &str) -> CoreResult<String> {
        tokio::fs::create_dir_all(&self.artifact_dir)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        session
            .backend
            .configure_downloads(&self.artifact_dir)
            .await?;
        let backend_subscription_id = session
            .backend
            .subscribe(EventFilter {
                methods: vec![
                    "Browser.downloadWillBegin".into(),
                    "Browser.downloadProgress".into(),
                ],
                session_id: None,
            })
            .await?;
        let subscription_id = opaque_id("sub");
        session
            .subscriptions
            .insert(subscription_id.clone(), backend_subscription_id);
        Ok(subscription_id)
    }

    pub async fn downloads_collect(
        &self,
        browser_session_id: &str,
        subscription_id: &str,
        after_sequence: u64,
        wait: Duration,
    ) -> CoreResult<DownloadCollection> {
        let batch = self
            .cdp_events(
                browser_session_id,
                subscription_id,
                after_sequence,
                1_000,
                wait,
            )
            .await?;
        let session = self.session(browser_session_id).await?;
        let completions = {
            let mut session = session.lock().await;
            for event in &batch.events {
                let Some(guid) = event.params.get("guid").and_then(Value::as_str) else {
                    continue;
                };
                let download = session.downloads.entry(guid.to_owned()).or_default();
                if event.method == "Browser.downloadWillBegin" {
                    download.suggested_filename = event
                        .params
                        .get("suggestedFilename")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
            }
            batch
                .events
                .iter()
                .filter(|event| {
                    event.method == "Browser.downloadProgress"
                        && event.params.get("state").and_then(Value::as_str) == Some("completed")
                })
                .filter_map(|event| {
                    let guid = event.params.get("guid")?.as_str()?.to_owned();
                    let download = session.downloads.entry(guid.clone()).or_default();
                    if download.artifact.is_some() {
                        return None;
                    }
                    let source = event
                        .params
                        .get("filePath")
                        .and_then(Value::as_str)
                        .map(PathBuf::from)
                        .unwrap_or_else(|| self.artifact_dir.join(&guid));
                    let name = download
                        .suggested_filename
                        .clone()
                        .unwrap_or_else(|| "download.bin".into());
                    Some((guid, source, name))
                })
                .collect::<Vec<_>>()
        };

        for (guid, source, name) in completions {
            let artifact = self.register_download(&source, &name).await?;
            session
                .lock()
                .await
                .downloads
                .entry(guid)
                .or_default()
                .artifact = Some(artifact);
        }
        let artifacts = session
            .lock()
            .await
            .downloads
            .values()
            .filter_map(|download| download.artifact.clone())
            .collect();
        Ok(DownloadCollection {
            events: batch,
            artifacts,
        })
    }

    pub async fn downloads_stop(
        &self,
        browser_session_id: &str,
        subscription_id: &str,
    ) -> CoreResult<()> {
        self.cdp_unsubscribe(browser_session_id, subscription_id)
            .await?;
        let session = self.session(browser_session_id).await?;
        session.lock().await.backend.disable_downloads().await
    }

    pub async fn cdp_targets(&self, browser_session_id: &str) -> CoreResult<Vec<TabSummary>> {
        self.tabs(browser_session_id).await
    }

    pub async fn cdp_attach(&self, browser_session_id: &str, tab_id: &str) -> CoreResult<String> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let target_id = session
            .tabs
            .get(tab_id)
            .ok_or_else(|| CoreError::NotFound(format!("tab {tab_id}")))?
            .backend_target_id
            .clone();
        let backend_session_id = session.backend.attach_target(&target_id).await?;
        let cdp_session_id = opaque_id("cs");
        session
            .cdp_sessions
            .insert(cdp_session_id.clone(), backend_session_id);
        Ok(cdp_session_id)
    }

    pub async fn cdp_detach(
        &self,
        browser_session_id: &str,
        cdp_session_id: &str,
    ) -> CoreResult<()> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let backend_session_id = session
            .cdp_sessions
            .remove(cdp_session_id)
            .ok_or_else(|| CoreError::NotFound(format!("CDP session {cdp_session_id}")))?;
        session.backend.detach_target(&backend_session_id).await
    }

    pub async fn cdp_send(
        &self,
        browser_session_id: &str,
        cdp_session_id: Option<&str>,
        target: &str,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> CoreResult<Value> {
        validate_cdp_command(method, &params)
            .map_err(|error| CoreError::InvalidRequest(error.to_string()))?;
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let backend_session_id = if target == "browser" {
            None
        } else if let Some(cdp_session_id) = cdp_session_id {
            Some(
                session
                    .cdp_sessions
                    .get(cdp_session_id)
                    .ok_or_else(|| CoreError::NotFound(format!("CDP session {cdp_session_id}")))?
                    .clone(),
            )
        } else {
            Some(session.tab_session(None)?.1)
        };
        session
            .backend
            .send_command(
                backend_session_id.as_ref(),
                method,
                params,
                timeout.clamp(Duration::from_millis(1), Duration::from_secs(15)),
            )
            .await
    }

    pub async fn cdp_subscribe(
        &self,
        browser_session_id: &str,
        cdp_session_id: Option<&str>,
        methods: Vec<String>,
    ) -> CoreResult<String> {
        if methods.is_empty() || methods.len() > 32 {
            return Err(CoreError::InvalidRequest(
                "methods must contain between 1 and 32 CDP event names".into(),
            ));
        }
        for method in &methods {
            validate_cdp_command(method, &json!({}))
                .map_err(|error| CoreError::InvalidRequest(error.to_string()))?;
        }
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let backend_session_id = if let Some(cdp_session_id) = cdp_session_id {
            session
                .cdp_sessions
                .get(cdp_session_id)
                .cloned()
                .ok_or_else(|| CoreError::NotFound(format!("CDP session {cdp_session_id}")))?
        } else {
            session.tab_session(None)?.1
        };
        let backend_subscription_id = session
            .backend
            .subscribe(EventFilter {
                methods,
                session_id: Some(backend_session_id),
            })
            .await?;
        let subscription_id = opaque_id("sub");
        session
            .subscriptions
            .insert(subscription_id.clone(), backend_subscription_id);
        Ok(subscription_id)
    }

    pub async fn cdp_events(
        &self,
        browser_session_id: &str,
        subscription_id: &str,
        after_sequence: u64,
        max_events: usize,
        wait: Duration,
    ) -> CoreResult<EventBatch> {
        let session = self.session(browser_session_id).await?;
        let (backend, backend_subscription_id) = {
            let session = session.lock().await;
            let backend_subscription_id = session
                .subscriptions
                .get(subscription_id)
                .cloned()
                .ok_or_else(|| CoreError::NotFound(format!("subscription {subscription_id}")))?;
            (session.backend.clone(), backend_subscription_id)
        };
        backend
            .poll_events(
                &backend_subscription_id,
                after_sequence,
                max_events.clamp(1, 1_000),
                wait.min(Duration::from_secs(5)),
            )
            .await
    }

    pub async fn cdp_unsubscribe(
        &self,
        browser_session_id: &str,
        subscription_id: &str,
    ) -> CoreResult<()> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let backend_subscription_id = session
            .subscriptions
            .remove(subscription_id)
            .ok_or_else(|| CoreError::NotFound(format!("subscription {subscription_id}")))?;
        session.backend.unsubscribe(&backend_subscription_id).await
    }

    pub async fn route_add(
        &self,
        browser_session_id: &str,
        tab_id: Option<&str>,
        rule: RouteRule,
    ) -> CoreResult<RouteDescriptor> {
        validate_route_rule(&rule)?;
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let (tab_id, backend_session_id) = session.tab_session(tab_id)?;
        let backend_route_id = session
            .backend
            .add_route(&backend_session_id, rule.clone())
            .await?;
        let route_id = opaque_id("route");
        let descriptor = RouteDescriptor {
            route_id: route_id.clone(),
            tab_id,
            rule,
        };
        session.routes.insert(
            route_id,
            RouteState {
                backend_route_id,
                descriptor: descriptor.clone(),
            },
        );
        Ok(descriptor)
    }

    pub async fn route_list(&self, browser_session_id: &str) -> CoreResult<Vec<RouteDescriptor>> {
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let mut routes = session
            .routes
            .values()
            .map(|route| route.descriptor.clone())
            .collect::<Vec<_>>();
        routes.sort_by(|left, right| left.route_id.cmp(&right.route_id));
        Ok(routes)
    }

    pub async fn route_remove(&self, browser_session_id: &str, route_id: &str) -> CoreResult<()> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let route = session
            .routes
            .remove(route_id)
            .ok_or_else(|| CoreError::NotFound(format!("route {route_id}")))?;
        session.backend.remove_route(&route.backend_route_id).await
    }

    pub async fn route_clear(&self, browser_session_id: &str) -> CoreResult<usize> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let routes = session
            .routes
            .drain()
            .map(|(_, route)| route.backend_route_id)
            .collect::<Vec<_>>();
        let count = routes.len();
        for route_id in routes {
            session.backend.remove_route(&route_id).await?;
        }
        Ok(count)
    }

    pub async fn har_start(&self, browser_session_id: &str) -> CoreResult<String> {
        self.cdp_subscribe(
            browser_session_id,
            None,
            vec![
                "Network.requestWillBeSent".into(),
                "Network.requestWillBeSentExtraInfo".into(),
                "Network.responseReceived".into(),
                "Network.responseReceivedExtraInfo".into(),
                "Network.loadingFinished".into(),
                "Network.loadingFailed".into(),
            ],
        )
        .await
    }

    pub async fn har_stop(
        &self,
        browser_session_id: &str,
        subscription_id: &str,
    ) -> CoreResult<ArtifactDescriptor> {
        let batch = self
            .cdp_events(
                browser_session_id,
                subscription_id,
                0,
                10_000,
                Duration::ZERO,
            )
            .await?;
        self.cdp_unsubscribe(browser_session_id, subscription_id)
            .await?;
        let har = build_har(batch);
        let bytes = serde_json::to_vec_pretty(&har)
            .map_err(|error| CoreError::Backend(format!("failed to serialize HAR: {error}")))?;
        self.write_artifact("network.har", "application/json", &bytes)
            .await
    }

    async fn session(&self, browser_session_id: &str) -> CoreResult<Arc<Mutex<BrowserSession>>> {
        self.sessions
            .read()
            .await
            .get(browser_session_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound(format!("browser session {browser_session_id}")))
    }

    async fn resolve_ref(
        &self,
        browser_session_id: &str,
        reference: &str,
    ) -> CoreResult<(Arc<dyn BrowserBackend>, BackendSessionId, String)> {
        let session = self.session(browser_session_id).await?;
        let session = session.lock().await;
        let element = session.element_refs.get(reference).ok_or_else(|| {
            CoreError::NotFound(format!(
                "element reference {reference}; take a new snapshot"
            ))
        })?;
        if element.generation != session.ref_generation {
            return Err(CoreError::InvalidRequest(
                "element reference is stale; take a new snapshot".into(),
            ));
        }
        let tab = session
            .tabs
            .get(&element.tab_id)
            .ok_or_else(|| CoreError::NotFound(format!("tab {}", element.tab_id)))?;
        Ok((
            session.backend.clone(),
            tab.backend_session_id.clone(),
            element.selector.clone(),
        ))
    }

    async fn write_artifact(
        &self,
        name: &str,
        mime_type: &str,
        bytes: &[u8],
    ) -> CoreResult<ArtifactDescriptor> {
        tokio::fs::create_dir_all(&self.artifact_dir)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        let artifact_id = opaque_id("artifact");
        let safe_name = format!("{artifact_id}-{name}");
        let path = self.artifact_dir.join(&safe_name);
        ensure_within(&self.artifact_dir, &path)?;
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        let sha256 = format!("{:x}", Sha256::digest(bytes));
        Ok(ArtifactDescriptor {
            artifact_id,
            relative_path: safe_name,
            display_name: name.to_owned(),
            media_type: mime_type.to_owned(),
            size_bytes: bytes.len() as u64,
            sha256,
        })
    }

    async fn register_download(
        &self,
        source: &Path,
        suggested_name: &str,
    ) -> CoreResult<ArtifactDescriptor> {
        let mut attempts = 0;
        while tokio::fs::metadata(source).await.is_err() && attempts < 20 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            attempts += 1;
        }
        let root = tokio::fs::canonicalize(&self.artifact_dir)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        let source = tokio::fs::canonicalize(source)
            .await
            .map_err(|error| CoreError::Io(format!("download file is unavailable: {error}")))?;
        if !source.starts_with(&root) {
            return Err(CoreError::InvalidRequest(
                "download path escaped artifact directory".into(),
            ));
        }
        let metadata = tokio::fs::metadata(&source)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
            return Err(CoreError::InvalidRequest(
                "download is not a regular file or exceeds 256 MiB".into(),
            ));
        }
        let bytes = tokio::fs::read(&source)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        let artifact_id = opaque_id("artifact");
        let sanitized_name = sanitize_artifact_name(suggested_name);
        let stored_name = format!("{artifact_id}-{sanitized_name}");
        let destination = self.artifact_dir.join(&stored_name);
        ensure_within(&self.artifact_dir, &destination)?;
        if source != destination {
            tokio::fs::rename(&source, &destination)
                .await
                .map_err(|error| CoreError::Io(error.to_string()))?;
        }
        Ok(ArtifactDescriptor {
            artifact_id,
            relative_path: stored_name,
            display_name: sanitized_name.clone(),
            media_type: mime_type_for_name(&sanitized_name).into(),
            size_bytes: metadata.len(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        })
    }

    async fn resolve_file_grant(&self, file_grant_id: &str) -> CoreResult<PathBuf> {
        if file_grant_id.is_empty()
            || file_grant_id.len() > 128
            || !file_grant_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err(CoreError::InvalidRequest(
                "file_grant_id has an invalid format".into(),
            ));
        }
        let grant_dir = env::var_os("CHATOS_PLUGIN_FILE_GRANT_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| {
                CoreError::Unsupported(
                    "CHATOS_PLUGIN_FILE_GRANT_DIR was not supplied by Local Connector".into(),
                )
            })?;
        let descriptor_path = grant_dir.join(format!("{file_grant_id}.json"));
        ensure_within(&grant_dir, &descriptor_path)?;
        let descriptor_bytes = tokio::fs::read(&descriptor_path)
            .await
            .map_err(|error| CoreError::NotFound(format!("file grant {file_grant_id}: {error}")))?;
        if descriptor_bytes.len() > 64 * 1024 {
            return Err(CoreError::InvalidRequest(
                "file grant descriptor exceeds 64 KiB".into(),
            ));
        }
        let descriptor: FileGrantDescriptor = serde_json::from_slice(&descriptor_bytes)
            .map_err(|error| CoreError::InvalidRequest(format!("invalid file grant: {error}")))?;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if descriptor.expires_at_unix_ms <= now_ms {
            return Err(CoreError::InvalidRequest(format!(
                "file grant {file_grant_id} has expired"
            )));
        }
        let path = tokio::fs::canonicalize(&descriptor.path)
            .await
            .map_err(|error| {
                CoreError::NotFound(format!("granted file is unavailable: {error}"))
            })?;
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
            return Err(CoreError::InvalidRequest(
                "granted upload is not a regular file or exceeds 128 MiB".into(),
            ));
        }
        if metadata.len() != descriptor.size {
            return Err(CoreError::InvalidRequest(
                "granted upload size no longer matches its descriptor".into(),
            ));
        }
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        if !sha256.eq_ignore_ascii_case(&descriptor.sha256) {
            return Err(CoreError::InvalidRequest(
                "granted upload SHA-256 no longer matches its descriptor".into(),
            ));
        }
        Ok(path)
    }
}

impl BrowserSession {
    fn summary(&self, browser_session_id: &str) -> BrowserSessionSummary {
        BrowserSessionSummary {
            browser_session_id: browser_session_id.to_owned(),
            mode: self.mode,
            state: "open",
            active_tab_id: self.active_tab_id.clone(),
            tab_count: self.tabs.len(),
            browser: self.browser.clone(),
        }
    }

    fn tab_session(&self, tab_id: Option<&str>) -> CoreResult<(String, BackendSessionId)> {
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

    fn invalidate_refs(&mut self) {
        self.ref_generation = self.ref_generation.wrapping_add(1);
        self.element_refs.clear();
    }
}

async fn evaluate_value(
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

fn virtual_cursor_move_script(selector: &str) -> String {
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
      willChange: 'transform', transition: 'transform 120ms cubic-bezier(.2,.8,.2,1)'
    }});
    const root = host.attachShadow ? host.attachShadow({{mode:'open'}}) : host;
    root.innerHTML = `<style>
      .pointer {{ position:absolute; left:-2px; top:-2px; width:26px; height:30px; filter:drop-shadow(0 1px 2px rgba(0,0,0,.45)); }}
      .pulse {{ position:absolute; left:-12px; top:-12px; width:24px; height:24px; border:2px solid #1677ff; border-radius:999px; opacity:0; transform:scale(.35); }}
      .pulse.active {{ animation:chatos-click 360ms ease-out; }}
      @keyframes chatos-click {{ 0% {{opacity:.95;transform:scale(.35)}} 100% {{opacity:0;transform:scale(1.65)}} }}
    </style><svg class="pointer" viewBox="0 0 26 30" xmlns="http://www.w3.org/2000/svg"><path d="M2 2v21.1l5.7-5.1 4.1 9.1 4.2-1.9-4.1-8.9h8.2L2 2z" fill="#111827" stroke="white" stroke-width="1.8" stroke-linejoin="round"/></svg><span class="pulse"></span>`;
    (document.documentElement || document.body).appendChild(host);
    const startX = Math.max(0, innerWidth / 2);
    const startY = Math.max(0, innerHeight / 2);
    host.style.transition = 'none';
    host.style.transform = `translate3d(${{startX}}px, ${{startY}}px, 0)`;
    host.getBoundingClientRect();
    host.style.transition = 'transform 120ms cubic-bezier(.2,.8,.2,1)';
  }}
  host.style.opacity = '1';
  host.style.transform = `translate3d(${{x}}px, ${{y}}px, 0)`;
  host.dataset.x = String(x);
  host.dataset.y = String(y);
  await new Promise(resolve => setTimeout(resolve, 135));
  return {{x, y}};
}})()"##,
        selector = selector,
        cursor_id = serde_json::to_string(VIRTUAL_CURSOR_ID).unwrap()
    )
}

fn virtual_cursor_pulse_script() -> String {
    format!(
        r#"(() => {{
  const host = document.getElementById({cursor_id});
  if (!host || !host.shadowRoot) return false;
  const pulse = host.shadowRoot.querySelector('.pulse');
  if (!pulse) return false;
  pulse.classList.remove('active');
  void pulse.offsetWidth;
  pulse.classList.add('active');
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
        assert!(script.contains("return {x, y}"));
    }

    #[test]
    fn cursor_pulse_uses_the_owned_shadow_root() {
        let script = virtual_cursor_pulse_script();
        assert!(script.contains(VIRTUAL_CURSOR_ID));
        assert!(script.contains("querySelector('.pulse')"));
    }
}

async fn read_title(
    backend: &dyn BrowserBackend,
    session_id: &BackendSessionId,
) -> CoreResult<String> {
    evaluate_value(backend, session_id, "document.title")
        .await?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Backend("document title is not a string".into()))
}

async fn wait_until_ready(
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

fn opaque_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn ensure_within(root: &Path, path: &Path) -> CoreResult<()> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(CoreError::InvalidRequest(
            "artifact path escaped artifact directory".into(),
        ))
    }
}

fn sanitize_artifact_name(name: &str) -> String {
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

fn mime_type_for_name(name: &str) -> &'static str {
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

fn validate_route_rule(rule: &RouteRule) -> CoreResult<()> {
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

fn build_har(batch: EventBatch) -> Value {
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

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use browser_cdp_protocol::{
        BrowserDescriptor, BrowserMode, EventBatch, EventFilter, OpenBrowserRequest, RouteRule,
        TargetDescriptor,
    };
    use tokio::sync::Notify;

    use super::*;

    struct WaitingBackend {
        first_evaluation: Notify,
    }

    #[async_trait]
    impl BrowserBackend for WaitingBackend {
        async fn open(&self, request: OpenBrowserRequest) -> CoreResult<BrowserDescriptor> {
            Ok(BrowserDescriptor {
                mode: request.mode,
                product: "test-browser".into(),
                user_agent: "test-agent".into(),
                capabilities: Vec::new(),
            })
        }

        async fn list_targets(&self) -> CoreResult<Vec<TargetDescriptor>> {
            Ok(vec![TargetDescriptor {
                id: "target-1".into(),
                title: Some("Test".into()),
                url: Some("about:blank".into()),
                kind: "page".into(),
            }])
        }

        async fn create_target(&self, url: &str) -> CoreResult<TargetDescriptor> {
            Ok(TargetDescriptor {
                id: "target-created".into(),
                title: None,
                url: Some(url.into()),
                kind: "page".into(),
            })
        }

        async fn close_target(&self, _target_id: &str) -> CoreResult<()> {
            Ok(())
        }

        async fn attach_target(&self, _target_id: &str) -> CoreResult<BackendSessionId> {
            Ok(BackendSessionId("backend-session-1".into()))
        }

        async fn detach_target(&self, _session_id: &BackendSessionId) -> CoreResult<()> {
            Ok(())
        }

        async fn send_command(
            &self,
            _session_id: Option<&BackendSessionId>,
            method: &str,
            _params: Value,
            _timeout: Duration,
        ) -> CoreResult<Value> {
            assert_eq!(method, "Runtime.evaluate");
            self.first_evaluation.notify_waiters();
            Ok(json!({ "result": { "value": false } }))
        }

        async fn subscribe(&self, _filter: EventFilter) -> CoreResult<String> {
            unreachable!("not used by this test")
        }

        async fn poll_events(
            &self,
            _subscription_id: &str,
            _after_sequence: u64,
            _max_events: usize,
            _wait: Duration,
        ) -> CoreResult<EventBatch> {
            unreachable!("not used by this test")
        }

        async fn unsubscribe(&self, _subscription_id: &str) -> CoreResult<()> {
            unreachable!("not used by this test")
        }

        async fn add_route(
            &self,
            _session_id: &BackendSessionId,
            _rule: RouteRule,
        ) -> CoreResult<String> {
            unreachable!("not used by this test")
        }

        async fn remove_route(&self, _route_id: &str) -> CoreResult<()> {
            unreachable!("not used by this test")
        }

        async fn close(&self) -> CoreResult<()> {
            Ok(())
        }
    }

    struct WaitingBackendFactory {
        backend: Arc<WaitingBackend>,
    }

    #[async_trait]
    impl BrowserBackendFactory for WaitingBackendFactory {
        fn supports(&self, mode: BrowserMode) -> bool {
            mode == BrowserMode::Managed
        }

        async fn create(&self, _mode: BrowserMode) -> CoreResult<Arc<dyn BrowserBackend>> {
            Ok(self.backend.clone())
        }
    }

    #[tokio::test]
    async fn wait_does_not_hold_session_lock_while_polling() {
        let backend = Arc::new(WaitingBackend {
            first_evaluation: Notify::new(),
        });
        let runtime = Arc::new(BrowserRuntime::new(
            vec![Arc::new(WaitingBackendFactory {
                backend: backend.clone(),
            })],
            std::env::temp_dir(),
        ));
        let session = runtime
            .open_session(OpenBrowserRequest {
                mode: BrowserMode::Managed,
                headless: true,
                persistent_profile: false,
                session_name: None,
            })
            .await
            .expect("open fake browser session");

        let waiting_runtime = runtime.clone();
        let browser_session_id = session.browser_session_id.clone();
        let wait_task = tokio::spawn(async move {
            waiting_runtime
                .wait(
                    &browser_session_id,
                    Some("#never-matches"),
                    None,
                    Duration::from_millis(400),
                )
                .await
        });
        backend.first_evaluation.notified().await;

        let status = tokio::time::timeout(
            Duration::from_millis(100),
            runtime.session_status(&session.browser_session_id),
        )
        .await
        .expect("session status must not wait for browser_wait's polling loop")
        .expect("read session status");
        assert_eq!(status.browser_session_id, session.browser_session_id);

        let wait_error = wait_task
            .await
            .expect("join browser_wait task")
            .expect_err("the fake selector never matches");
        assert!(matches!(wait_error, CoreError::Timeout(ref name) if name == "browser_wait"));
    }
}
