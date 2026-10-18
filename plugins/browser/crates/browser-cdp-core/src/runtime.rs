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

#[path = "runtime_cdp.rs"]
mod cdp;
#[path = "runtime_operations.rs"]
mod operations;
#[path = "runtime_scripts.rs"]
mod scripts;
#[path = "runtime_support.rs"]
mod support;

use scripts::*;
use support::*;

use crate::{BrowserBackend, BrowserBackendFactory};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(15);
const VIRTUAL_CURSOR_ID: &str = "__chatos_virtual_mouse__";
const VIRTUAL_CURSOR_MOVE_MS: u64 = 280;
const VIRTUAL_CURSOR_CLICK_HOLD_MS: u64 = 240;

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
    pub href: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub ready_state: String,
    pub document_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageSnapshot {
    #[serde(flatten)]
    pub page: PageState,
    pub text: String,
    pub text_truncated: bool,
    pub nodes: Vec<SnapshotNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadCollection {
    pub events: EventBatch,
    pub artifacts: Vec<ArtifactDescriptor>,
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
    href: Option<String>,
    page_url: String,
    document_id: String,
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
    virtual_cursor_scripts: HashMap<String, String>,
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
            virtual_cursor_scripts: HashMap::new(),
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
        let mut session = session.lock().await;
        let targets = session.backend.list_targets().await?;
        for tab in session.tabs.values_mut() {
            if let Some(target) = targets.iter().find(|target| target.id == tab.backend_target_id) {
                tab.url = target.url.clone();
                tab.title = target.title.clone();
            } else {
                tab.url = None;
                tab.title = None;
            }
        }
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
        // Invalidate before the command, including timeout/error cases.
        session.invalidate_refs();
        let result = session
            .backend
            .send_command(
                Some(&backend_session_id),
                "Page.navigate",
                json!({ "url": url }),
                timeout.min(Duration::from_secs(60)),
            )
            .await?;
        if let Some(error) = result.get("errorText").and_then(Value::as_str) {
            return Err(CoreError::Backend(format!("navigation rejected: {error}")));
        }
        wait_until_ready(session.backend.as_ref(), &backend_session_id, timeout).await?;
        let page = read_page_state(session.backend.as_ref(), &backend_session_id).await?;
        if let Some(tab) = session.tabs.get_mut(&tab_id) {
            tab.url = Some(page.url.clone());
            tab.title = Some(page.title.clone());
        }
        Ok(json!({"navigation": result, "page": page, "refs_invalidated": true}))
    }

    pub async fn snapshot(
        &self,
        browser_session_id: &str,
        tab_id: Option<&str>,
    ) -> CoreResult<PageSnapshot> {
        let session = self.session(browser_session_id).await?;
        let mut session = session.lock().await;
        let (tab_id, backend_session_id) = session.tab_session(tab_id)?;
        let value = evaluate_value(
            session.backend.as_ref(),
            &backend_session_id,
            SNAPSHOT_SCRIPT,
        )
        .await?;
        let raw: RawPageSnapshot = serde_json::from_value(value)
            .map_err(|error| CoreError::Backend(format!("invalid snapshot response: {error}")))?;
        session.invalidate_refs();
        let generation = session.ref_generation;
        if let Some(tab) = session.tabs.get_mut(&tab_id) {
            tab.url = Some(raw.page.url.clone());
            tab.title = Some(raw.page.title.clone());
        }
        let mut snapshots = Vec::with_capacity(raw.nodes.len());
        for node in raw.nodes {
            let reference = opaque_id("ref");
            session.element_refs.insert(
                reference.clone(),
                ElementReference {
                    tab_id: tab_id.clone(),
                    generation,
                    selector: node.selector,
                    href: node.href.clone(),
                    page_url: raw.page.url.clone(),
                    document_id: raw.page.document_id.clone(),
                },
            );
            snapshots.push(SnapshotNode {
                reference,
                role: node.role,
                name: node.name,
                value: node.value,
                disabled: node.disabled,
                href: node.href,
            });
        }
        Ok(PageSnapshot { page: raw.page, text: raw.text, text_truncated: raw.text_truncated, nodes: snapshots })
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
            .nodes
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
        let (backend, backend_session_id, selector, href, mode) =
            self.resolve_ref(browser_session_id, reference).await?;
        let before = read_page_state(backend.as_ref(), &backend_session_id).await?;
        // chrome.debugger can lose the response to Input.dispatchMouseEvent when
        // mouseReleased immediately navigates the tab. A safe href came from the
        // exact current snapshot/ref, so following it is both provenance-preserving
        // and more reliable than retrying an ambiguously timed-out mouse click.
        if mode == BrowserMode::ChromeExtension {
            if let Some(href) = href {
                validate_navigation_url(&href)
                    .map_err(|error| CoreError::InvalidRequest(error.to_string()))?;
                self.session(browser_session_id).await?.lock().await.invalidate_refs();
                let navigation = backend
                    .send_command(
                        Some(&backend_session_id),
                        "Page.navigate",
                        json!({ "url": href }),
                        COMMAND_TIMEOUT,
                    )
                    .await?;
                if let Some(error) = navigation.get("errorText").and_then(Value::as_str) {
                    return Err(CoreError::Backend(format!("navigation rejected: {error}")));
                }
                let deadline = Instant::now() + Duration::from_millis(1200);
                let mut after = None;
                loop {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    if let Ok(page) = read_page_state(backend.as_ref(), &backend_session_id).await {
                        let changed = page.url != before.url || page.document_id != before.document_id;
                        let ready = page.ready_state == "complete";
                        after = Some(page);
                        if (changed && ready) || Instant::now() >= deadline { break; }
                    } else if Instant::now() >= deadline { break; }
                }
                if let Some(page) = &after {
                    let session = self.session(browser_session_id).await?;
                    let mut session = session.lock().await;
                    if let Some(tab) = session.tabs.values_mut().find(|tab| tab.backend_session_id.0 == backend_session_id.0) {
                        tab.url = Some(page.url.clone());
                        tab.title = Some(page.title.clone());
                    }
                }
                let navigated = after.as_ref().map(|page| page.url != before.url || page.document_id != before.document_id);
                return Ok(json!({
                    "clicked": true,
                    "interaction": "follow_snapshot_link",
                    "before": before,
                    "after": after,
                    "navigated": navigated,
                    "refs_invalidated": true,
                    "next_action": "Take a fresh browser_snapshot; if the page is still loading, wait and snapshot again."
                }));
            }
        }
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

        // Register the last cursor position before dispatching the click so a
        // navigation caused by mouseReleased recreates the same cursor in the
        // next document instead of making it appear to vanish.
        let _ = self
            .persist_virtual_cursor(
                browser_session_id,
                backend.as_ref(),
                &backend_session_id,
                x,
                y,
            )
            .await;

        self.session(browser_session_id).await?.lock().await.invalidate_refs();
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
        // Keep the pressed cursor visible before mouseReleased triggers a
        // navigation and destroys the current page overlay.
        tokio::time::sleep(Duration::from_millis(VIRTUAL_CURSOR_CLICK_HOLD_MS)).await;
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
        // Allow asynchronous navigation/SPA changes to begin. Failure to observe
        // the result is not failure to dispatch: callers must not repeat a write.
        let deadline = Instant::now() + Duration::from_millis(1200);
        let mut after = None;
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(page) = read_page_state(backend.as_ref(), &backend_session_id).await {
                let changed = page.url != before.url || page.document_id != before.document_id;
                let ready = page.ready_state == "complete";
                after = Some(page);
                if (changed && ready) || Instant::now() >= deadline { break; }
            } else if Instant::now() >= deadline { break; }
        }
        if let Some(page) = &after {
            let session = self.session(browser_session_id).await?;
            let mut session = session.lock().await;
            if let Some(tab) = session.tabs.values_mut().find(|tab| tab.backend_session_id.0 == backend_session_id.0) {
                tab.url = Some(page.url.clone());
                tab.title = Some(page.title.clone());
            }
        }
        let navigated = after.as_ref().map(|page| page.url != before.url || page.document_id != before.document_id);
        Ok(json!({"clicked": true, "before": before, "after": after, "navigated": navigated,
            "refs_invalidated": true, "next_action": "Take a fresh browser_snapshot; if the page is still loading, wait and snapshot again. Do not repeat a consequential click just because observation is pending."}))
    }

    async fn persist_virtual_cursor(
        &self,
        browser_session_id: &str,
        backend: &dyn BrowserBackend,
        backend_session_id: &BackendSessionId,
        x: f64,
        y: f64,
    ) -> CoreResult<()> {
        let response = backend
            .send_command(
                Some(backend_session_id),
                "Page.addScriptToEvaluateOnNewDocument",
                json!({"source": virtual_cursor_restore_script(x, y)}),
                COMMAND_TIMEOUT,
            )
            .await?;
        let Some(identifier) = response.get("identifier").and_then(Value::as_str) else {
            return Ok(());
        };
        let session = self.session(browser_session_id).await?;
        let previous = session
            .lock()
            .await
            .virtual_cursor_scripts
            .insert(backend_session_id.0.clone(), identifier.to_owned());
        if let Some(previous) = previous {
            let _ = backend
                .send_command(
                    Some(backend_session_id),
                    "Page.removeScriptToEvaluateOnNewDocument",
                    json!({"identifier": previous}),
                    COMMAND_TIMEOUT,
                )
                .await;
        }
        Ok(())
    }

    pub async fn type_text(
        &self,
        browser_session_id: &str,
        reference: &str,
        text: &str,
        clear: bool,
    ) -> CoreResult<Value> {
        let (backend, backend_session_id, selector, _, _) =
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
}

#[cfg(test)]
include!("runtime_inline_tests.rs");
