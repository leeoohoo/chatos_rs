use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use browser_cdp_core::{BrowserBackend, BrowserBackendFactory, CoreError, CoreResult};
use browser_cdp_policy::{redact_sensitive_json, truncate_serializable};
use browser_cdp_protocol::{
    BackendSessionId, BrowserDescriptor, BrowserMode, CdpEvent, EventBatch, EventFilter,
    OpenBrowserRequest, RouteAction, RouteRule, TargetDescriptor,
};
use chromiumoxide::{
    Browser,
    browser::BrowserConfigBuilder,
    cdp::{
        IntoEventKind,
        browser_protocol::browser::{EventDownloadProgress, EventDownloadWillBegin},
        browser_protocol::fetch::EventRequestPaused,
        browser_protocol::network::{
            EventLoadingFailed, EventLoadingFinished, EventRequestWillBeSent,
            EventRequestWillBeSentExtraInfo, EventResponseReceived, EventResponseReceivedExtraInfo,
            EventWebSocketClosed, EventWebSocketCreated, EventWebSocketFrameError,
            EventWebSocketFrameReceived, EventWebSocketFrameSent,
            EventWebSocketHandshakeResponseReceived, EventWebSocketWillSendHandshakeRequest,
        },
        browser_protocol::page::{EventJavascriptDialogClosed, EventJavascriptDialogOpening},
        js_protocol::runtime::{EventConsoleApiCalled, EventExceptionThrown},
    },
    page::Page,
};
use chromiumoxide_types::{Command, Method, MethodId};
use futures::StreamExt;
use serde::{Serialize, ser::Serializer};
use serde_json::Value;
use tokio::{
    sync::{Mutex, Notify, RwLock},
    task::JoinHandle,
};
use uuid::Uuid;

pub struct DirectBackendFactory {
    data_dir: PathBuf,
}

impl DirectBackendFactory {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }
}

#[async_trait]
impl BrowserBackendFactory for DirectBackendFactory {
    fn supports(&self, mode: BrowserMode) -> bool {
        mode == BrowserMode::Managed
    }

    async fn create(&self, _mode: BrowserMode) -> CoreResult<Arc<dyn BrowserBackend>> {
        Ok(Arc::new(DirectCdpBackend {
            data_dir: self.data_dir.clone(),
            state: Mutex::new(DirectState::default()),
        }))
    }
}

#[derive(Default)]
struct DirectState {
    browser: Option<Browser>,
    handler_task: Option<JoinHandle<()>>,
    sessions: HashMap<String, Page>,
    subscriptions: HashMap<String, DirectSubscription>,
    route_workers: HashMap<String, DirectRouteWorker>,
    route_index: HashMap<String, String>,
    profile_dir: Option<PathBuf>,
    persistent_profile: bool,
}

struct DirectRouteWorker {
    rules: Arc<RwLock<Vec<BackendRoute>>>,
    task: JoinHandle<()>,
}

#[derive(Clone)]
struct BackendRoute {
    id: String,
    rule: RouteRule,
}

struct DirectSubscription {
    queue: Arc<BoundedEventQueue>,
    tasks: Vec<JoinHandle<()>>,
}

struct QueuedEvent {
    event: CdpEvent,
    size: usize,
}

#[derive(Default)]
struct EventQueueState {
    events: VecDeque<QueuedEvent>,
    total_bytes: usize,
    latest_sequence: u64,
    dropped_event_count: u64,
}

#[derive(Default)]
struct BoundedEventQueue {
    state: Mutex<EventQueueState>,
    notify: Notify,
}

const MAX_EVENT_COUNT: usize = 10_000;
const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SINGLE_EVENT_CHARS: usize = 256 * 1024;

pub struct DirectCdpBackend {
    data_dir: PathBuf,
    state: Mutex<DirectState>,
}

#[async_trait]
impl BrowserBackend for DirectCdpBackend {
    async fn open(&self, request: OpenBrowserRequest) -> CoreResult<BrowserDescriptor> {
        if request.mode != BrowserMode::Managed {
            return Err(CoreError::Unsupported(
                "direct backend only supports managed mode".into(),
            ));
        }

        let profile_dir = if request.persistent_profile {
            self.data_dir.join("profiles").join("default")
        } else {
            self.data_dir
                .join("profiles")
                .join(format!("session-{}", Uuid::new_v4().simple()))
        };
        tokio::fs::create_dir_all(&profile_dir)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;

        let mut builder = BrowserConfigBuilder::default()
            .user_data_dir(&profile_dir)
            .launch_timeout(Duration::from_secs(20))
            .request_timeout(Duration::from_secs(15));
        builder = if request.headless {
            builder.new_headless_mode()
        } else {
            builder.with_head()
        };
        let config = builder.build().map_err(|error| {
            CoreError::InvalidRequest(format!("invalid Chrome config: {error}"))
        })?;
        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|error| CoreError::Backend(error.to_string()))?;
        let handler_task = tokio::spawn(async move {
            while let Some(result) = handler.next().await {
                if let Err(error) = result {
                    tracing::warn!(%error, "Chrome CDP handler stopped with an error");
                    break;
                }
            }
        });

        let version = browser
            .version()
            .await
            .map_err(|error| CoreError::Backend(error.to_string()))?;
        let descriptor = BrowserDescriptor {
            mode: BrowserMode::Managed,
            product: version.product,
            user_agent: version.user_agent,
            capabilities: vec![
                "managed_browser".into(),
                "page_control".into(),
                "raw_cdp".into(),
                "screenshots".into(),
            ],
        };
        let mut state = self.state.lock().await;
        if state.browser.is_some() {
            return Err(CoreError::InvalidRequest("backend is already open".into()));
        }
        state.browser = Some(browser);
        state.handler_task = Some(handler_task);
        state.profile_dir = Some(profile_dir);
        state.persistent_profile = request.persistent_profile;
        Ok(descriptor)
    }

    async fn list_targets(&self) -> CoreResult<Vec<TargetDescriptor>> {
        let pages = {
            let state = self.state.lock().await;
            state
                .browser
                .as_ref()
                .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?
                .pages()
                .await
                .map_err(|error| CoreError::Backend(error.to_string()))?
        };
        let mut targets = Vec::with_capacity(pages.len());
        for page in pages {
            targets.push(page_descriptor(&page).await);
        }
        Ok(targets)
    }

    async fn create_target(&self, url: &str) -> CoreResult<TargetDescriptor> {
        let page = {
            let state = self.state.lock().await;
            state
                .browser
                .as_ref()
                .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?
                .new_page(url)
                .await
                .map_err(|error| CoreError::Backend(error.to_string()))?
        };
        Ok(page_descriptor(&page).await)
    }

    async fn close_target(&self, target_id: &str) -> CoreResult<()> {
        let page = self.find_page(target_id).await?;
        page.close()
            .await
            .map_err(|error| CoreError::Backend(error.to_string()))
    }

    async fn attach_target(&self, target_id: &str) -> CoreResult<BackendSessionId> {
        let page = self.find_page(target_id).await?;
        let backend_session_id = format!("backend_{}", Uuid::new_v4().simple());
        self.state
            .lock()
            .await
            .sessions
            .insert(backend_session_id.clone(), page);
        Ok(BackendSessionId(backend_session_id))
    }

    async fn detach_target(&self, session_id: &BackendSessionId) -> CoreResult<()> {
        self.state
            .lock()
            .await
            .sessions
            .remove(&session_id.0)
            .ok_or_else(|| CoreError::NotFound(format!("backend session {}", session_id.0)))?;
        Ok(())
    }

    async fn send_command(
        &self,
        session_id: Option<&BackendSessionId>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> CoreResult<Value> {
        let command = RawCommand {
            method: method.to_owned(),
            params,
        };
        let future = async {
            if let Some(session_id) = session_id {
                let page = self
                    .state
                    .lock()
                    .await
                    .sessions
                    .get(&session_id.0)
                    .cloned()
                    .ok_or_else(|| {
                        CoreError::NotFound(format!("backend session {}", session_id.0))
                    })?;
                page.execute(command)
                    .await
                    .map(|response| response.result)
                    .map_err(|error| CoreError::Backend(error.to_string()))
            } else {
                let state = self.state.lock().await;
                state
                    .browser
                    .as_ref()
                    .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?
                    .execute(command)
                    .await
                    .map(|response| response.result)
                    .map_err(|error| CoreError::Backend(error.to_string()))
            }
        };
        tokio::time::timeout(timeout, future)
            .await
            .map_err(|_| CoreError::Timeout(method.to_owned()))?
    }

    async fn subscribe(&self, filter: EventFilter) -> CoreResult<String> {
        let queue = Arc::new(BoundedEventQueue::default());
        if filter.session_id.is_none() {
            let mut tasks: Vec<JoinHandle<()>> = Vec::new();
            {
                let state = self.state.lock().await;
                let browser = state
                    .browser
                    .as_ref()
                    .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?;
                for method in &filter.methods {
                    let task = match method.as_str() {
                        "Browser.downloadWillBegin" => {
                            spawn_browser_event_listener::<EventDownloadWillBegin>(
                                browser,
                                method,
                                queue.clone(),
                            )
                            .await?
                        }
                        "Browser.downloadProgress" => {
                            spawn_browser_event_listener::<EventDownloadProgress>(
                                browser,
                                method,
                                queue.clone(),
                            )
                            .await?
                        }
                        _ => {
                            for task in tasks {
                                task.abort();
                            }
                            return Err(CoreError::Unsupported(format!(
                                "browser event {method} is not yet supported by the direct backend"
                            )));
                        }
                    };
                    tasks.push(task);
                }
            }
            let subscription_id = format!("backend_sub_{}", Uuid::new_v4().simple());
            self.state
                .lock()
                .await
                .subscriptions
                .insert(subscription_id.clone(), DirectSubscription { queue, tasks });
            return Ok(subscription_id);
        }

        let session_id = filter.session_id.as_ref().ok_or_else(|| {
            CoreError::InvalidRequest("page event subscription requires a session".into())
        })?;
        let page = self
            .state
            .lock()
            .await
            .sessions
            .get(&session_id.0)
            .cloned()
            .ok_or_else(|| CoreError::NotFound(format!("backend session {}", session_id.0)))?;
        let mut tasks: Vec<JoinHandle<()>> = Vec::new();

        if filter
            .methods
            .iter()
            .any(|method| method.starts_with("Runtime."))
        {
            execute_page_command(&page, "Runtime.enable", Value::Object(Default::default()))
                .await?;
        }
        if filter
            .methods
            .iter()
            .any(|method| method.starts_with("Network."))
        {
            execute_page_command(&page, "Network.enable", Value::Object(Default::default()))
                .await?;
        }

        for method in filter.methods {
            let task = match method.as_str() {
                "Runtime.consoleAPICalled" => {
                    spawn_event_listener::<EventConsoleApiCalled>(&page, &method, queue.clone())
                        .await?
                }
                "Runtime.exceptionThrown" => {
                    spawn_event_listener::<EventExceptionThrown>(&page, &method, queue.clone())
                        .await?
                }
                "Page.javascriptDialogOpening" => {
                    spawn_event_listener::<EventJavascriptDialogOpening>(
                        &page,
                        &method,
                        queue.clone(),
                    )
                    .await?
                }
                "Page.javascriptDialogClosed" => {
                    spawn_event_listener::<EventJavascriptDialogClosed>(
                        &page,
                        &method,
                        queue.clone(),
                    )
                    .await?
                }
                "Network.requestWillBeSent" => {
                    spawn_event_listener::<EventRequestWillBeSent>(&page, &method, queue.clone())
                        .await?
                }
                "Network.requestWillBeSentExtraInfo" => {
                    spawn_event_listener::<EventRequestWillBeSentExtraInfo>(
                        &page,
                        &method,
                        queue.clone(),
                    )
                    .await?
                }
                "Network.responseReceived" => {
                    spawn_event_listener::<EventResponseReceived>(&page, &method, queue.clone())
                        .await?
                }
                "Network.responseReceivedExtraInfo" => {
                    spawn_event_listener::<EventResponseReceivedExtraInfo>(
                        &page,
                        &method,
                        queue.clone(),
                    )
                    .await?
                }
                "Network.loadingFinished" => {
                    spawn_event_listener::<EventLoadingFinished>(&page, &method, queue.clone())
                        .await?
                }
                "Network.loadingFailed" => {
                    spawn_event_listener::<EventLoadingFailed>(&page, &method, queue.clone())
                        .await?
                }
                "Network.webSocketCreated" => {
                    spawn_event_listener::<EventWebSocketCreated>(&page, &method, queue.clone())
                        .await?
                }
                "Network.webSocketWillSendHandshakeRequest" => {
                    spawn_event_listener::<EventWebSocketWillSendHandshakeRequest>(
                        &page,
                        &method,
                        queue.clone(),
                    )
                    .await?
                }
                "Network.webSocketHandshakeResponseReceived" => {
                    spawn_event_listener::<EventWebSocketHandshakeResponseReceived>(
                        &page,
                        &method,
                        queue.clone(),
                    )
                    .await?
                }
                "Network.webSocketFrameSent" => {
                    spawn_event_listener::<EventWebSocketFrameSent>(&page, &method, queue.clone())
                        .await?
                }
                "Network.webSocketFrameReceived" => {
                    spawn_event_listener::<EventWebSocketFrameReceived>(
                        &page,
                        &method,
                        queue.clone(),
                    )
                    .await?
                }
                "Network.webSocketFrameError" => {
                    spawn_event_listener::<EventWebSocketFrameError>(&page, &method, queue.clone())
                        .await?
                }
                "Network.webSocketClosed" => {
                    spawn_event_listener::<EventWebSocketClosed>(&page, &method, queue.clone())
                        .await?
                }
                _ => {
                    for task in tasks {
                        task.abort();
                    }
                    return Err(CoreError::Unsupported(format!(
                        "CDP event {method} is not yet supported by the direct backend"
                    )));
                }
            };
            tasks.push(task);
        }

        let subscription_id = format!("backend_sub_{}", Uuid::new_v4().simple());
        self.state
            .lock()
            .await
            .subscriptions
            .insert(subscription_id.clone(), DirectSubscription { queue, tasks });
        Ok(subscription_id)
    }

    async fn poll_events(
        &self,
        subscription_id: &str,
        after_sequence: u64,
        max_events: usize,
        wait: Duration,
    ) -> CoreResult<EventBatch> {
        let queue = self
            .state
            .lock()
            .await
            .subscriptions
            .get(subscription_id)
            .map(|subscription| subscription.queue.clone())
            .ok_or_else(|| {
                CoreError::NotFound(format!("backend subscription {subscription_id}"))
            })?;
        Ok(queue.poll(after_sequence, max_events, wait).await)
    }

    async fn unsubscribe(&self, subscription_id: &str) -> CoreResult<()> {
        let subscription = self
            .state
            .lock()
            .await
            .subscriptions
            .remove(subscription_id)
            .ok_or_else(|| {
                CoreError::NotFound(format!("backend subscription {subscription_id}"))
            })?;
        for task in subscription.tasks {
            task.abort();
        }
        Ok(())
    }

    async fn add_route(
        &self,
        session_id: &BackendSessionId,
        rule: RouteRule,
    ) -> CoreResult<String> {
        let route_id = format!("backend_route_{}", Uuid::new_v4().simple());
        let existing_rules = {
            let mut state = self.state.lock().await;
            let rules = state
                .route_workers
                .get(&session_id.0)
                .map(|worker| worker.rules.clone());
            if rules.is_some() {
                state
                    .route_index
                    .insert(route_id.clone(), session_id.0.clone());
            }
            rules
        };
        if let Some(rules) = existing_rules {
            rules.write().await.push(BackendRoute {
                id: route_id.clone(),
                rule,
            });
            return Ok(route_id);
        }

        let page = self
            .state
            .lock()
            .await
            .sessions
            .get(&session_id.0)
            .cloned()
            .ok_or_else(|| CoreError::NotFound(format!("backend session {}", session_id.0)))?;
        execute_page_command(
            &page,
            "Fetch.enable",
            serde_json::json!({ "patterns": [{ "urlPattern": "*" }] }),
        )
        .await?;
        let rules = Arc::new(RwLock::new(vec![BackendRoute {
            id: route_id.clone(),
            rule,
        }]));
        let task = spawn_route_worker(page, rules.clone()).await?;
        let mut state = self.state.lock().await;
        state
            .route_workers
            .insert(session_id.0.clone(), DirectRouteWorker { rules, task });
        state
            .route_index
            .insert(route_id.clone(), session_id.0.clone());
        Ok(route_id)
    }

    async fn remove_route(&self, route_id: &str) -> CoreResult<()> {
        let rules = {
            let mut state = self.state.lock().await;
            let session_id = state
                .route_index
                .remove(route_id)
                .ok_or_else(|| CoreError::NotFound(format!("backend route {route_id}")))?;
            state
                .route_workers
                .get(&session_id)
                .map(|worker| worker.rules.clone())
                .ok_or_else(|| CoreError::NotFound(format!("route worker {session_id}")))?
        };
        rules.write().await.retain(|route| route.id != route_id);
        Ok(())
    }

    async fn configure_downloads(&self, download_dir: &std::path::Path) -> CoreResult<()> {
        let download_dir = tokio::fs::canonicalize(download_dir)
            .await
            .map_err(|error| CoreError::Io(error.to_string()))?;
        let state = self.state.lock().await;
        state
            .browser
            .as_ref()
            .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?
            .execute(RawCommand {
                method: "Browser.setDownloadBehavior".into(),
                params: serde_json::json!({
                    "behavior": "allowAndName",
                    "downloadPath": download_dir,
                    "eventsEnabled": true
                }),
            })
            .await
            .map(|_| ())
            .map_err(|error| CoreError::Backend(error.to_string()))
    }

    async fn disable_downloads(&self) -> CoreResult<()> {
        let state = self.state.lock().await;
        state
            .browser
            .as_ref()
            .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?
            .execute(RawCommand {
                method: "Browser.setDownloadBehavior".into(),
                params: serde_json::json!({ "behavior": "deny", "eventsEnabled": false }),
            })
            .await
            .map(|_| ())
            .map_err(|error| CoreError::Backend(error.to_string()))
    }

    async fn close(&self) -> CoreResult<()> {
        let (
            mut browser,
            handler_task,
            subscriptions,
            route_workers,
            profile_dir,
            persistent_profile,
        ) = {
            let mut state = self.state.lock().await;
            state.sessions.clear();
            state.route_index.clear();
            (
                state.browser.take(),
                state.handler_task.take(),
                state
                    .subscriptions
                    .drain()
                    .map(|(_, subscription)| subscription)
                    .collect::<Vec<_>>(),
                state
                    .route_workers
                    .drain()
                    .map(|(_, worker)| worker)
                    .collect::<Vec<_>>(),
                state.profile_dir.take(),
                state.persistent_profile,
            )
        };
        for subscription in subscriptions {
            for task in subscription.tasks {
                task.abort();
            }
        }
        for worker in route_workers {
            worker.task.abort();
        }
        if let Some(browser) = browser.as_mut() {
            let _ = browser.close().await;
            let _ = browser.wait().await;
        }
        if let Some(handler_task) = handler_task {
            handler_task.abort();
        }
        if !persistent_profile {
            if let Some(profile_dir) = profile_dir {
                let _ = tokio::fs::remove_dir_all(profile_dir).await;
            }
        }
        Ok(())
    }
}

impl DirectCdpBackend {
    async fn find_page(&self, target_id: &str) -> CoreResult<Page> {
        let pages = {
            let state = self.state.lock().await;
            state
                .browser
                .as_ref()
                .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?
                .pages()
                .await
                .map_err(|error| CoreError::Backend(error.to_string()))?
        };
        pages
            .into_iter()
            .find(|page| page.target_id().as_ref() == target_id)
            .ok_or_else(|| CoreError::NotFound(format!("backend target {target_id}")))
    }
}

impl BoundedEventQueue {
    async fn push(&self, method: String, mut params: Value) {
        redact_sensitive_json(&mut params);
        params = truncate_serializable(&params, MAX_SINGLE_EVENT_CHARS);
        let size = method.len() + serde_json::to_vec(&params).map_or(0, |bytes| bytes.len());
        let mut state = self.state.lock().await;
        state.latest_sequence = state.latest_sequence.wrapping_add(1);
        let sequence = state.latest_sequence;
        while state.events.len() >= MAX_EVENT_COUNT
            || state.total_bytes.saturating_add(size) > MAX_EVENT_BYTES
        {
            let Some(dropped) = state.events.pop_front() else {
                break;
            };
            state.total_bytes = state.total_bytes.saturating_sub(dropped.size);
            state.dropped_event_count = state.dropped_event_count.saturating_add(1);
        }
        state.events.push_back(QueuedEvent {
            event: CdpEvent {
                sequence,
                method,
                params,
            },
            size,
        });
        state.total_bytes = state.total_bytes.saturating_add(size);
        drop(state);
        self.notify.notify_waiters();
    }

    async fn poll(&self, after_sequence: u64, max_events: usize, wait: Duration) -> EventBatch {
        let notified = self.notify.notified();
        {
            let state = self.state.lock().await;
            let batch = state.batch(after_sequence, max_events);
            if !batch.events.is_empty() || wait.is_zero() {
                return batch;
            }
        }
        let _ = tokio::time::timeout(wait, notified).await;
        self.state.lock().await.batch(after_sequence, max_events)
    }
}

impl EventQueueState {
    fn batch(&self, after_sequence: u64, max_events: usize) -> EventBatch {
        EventBatch {
            events: self
                .events
                .iter()
                .filter(|queued| queued.event.sequence > after_sequence)
                .take(max_events)
                .map(|queued| queued.event.clone())
                .collect(),
            dropped_event_count: self.dropped_event_count,
            latest_sequence: self.latest_sequence,
        }
    }
}

async fn execute_page_command(page: &Page, method: &str, params: Value) -> CoreResult<Value> {
    page.execute(RawCommand {
        method: method.to_owned(),
        params,
    })
    .await
    .map(|response| response.result)
    .map_err(|error| CoreError::Backend(error.to_string()))
}

async fn spawn_event_listener<T>(
    page: &Page,
    method: &str,
    queue: Arc<BoundedEventQueue>,
) -> CoreResult<JoinHandle<()>>
where
    T: IntoEventKind + Serialize + Unpin + Send + Sync + 'static,
{
    let mut events = page
        .event_listener::<T>()
        .await
        .map_err(|error| CoreError::Backend(error.to_string()))?;
    let method = method.to_owned();
    Ok(tokio::spawn(async move {
        while let Some(event) = events.next().await {
            if let Ok(params) = serde_json::to_value(&*event) {
                queue.push(method.clone(), params).await;
            }
        }
    }))
}

async fn spawn_browser_event_listener<T>(
    browser: &Browser,
    method: &str,
    queue: Arc<BoundedEventQueue>,
) -> CoreResult<JoinHandle<()>>
where
    T: IntoEventKind + Serialize + Unpin + Send + Sync + 'static,
{
    let mut events = browser
        .event_listener::<T>()
        .await
        .map_err(|error| CoreError::Backend(error.to_string()))?;
    let method = method.to_owned();
    Ok(tokio::spawn(async move {
        while let Some(event) = events.next().await {
            if let Ok(params) = serde_json::to_value(&*event) {
                queue.push(method.clone(), params).await;
            }
        }
    }))
}

async fn spawn_route_worker(
    page: Page,
    rules: Arc<RwLock<Vec<BackendRoute>>>,
) -> CoreResult<JoinHandle<()>> {
    let mut events = page
        .event_listener::<EventRequestPaused>()
        .await
        .map_err(|error| CoreError::Backend(error.to_string()))?;
    Ok(tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let matched = rules
                .read()
                .await
                .iter()
                .rev()
                .find(|route| wildcard_match(&route.rule.url_pattern, &event.request.url))
                .cloned();
            let request_id = event.request_id.as_ref();
            let result = match matched.map(|route| route.rule.action) {
                Some(RouteAction::Abort) => {
                    execute_page_command(
                        &page,
                        "Fetch.failRequest",
                        serde_json::json!({
                            "requestId": request_id,
                            "errorReason": "BlockedByClient"
                        }),
                    )
                    .await
                }
                Some(RouteAction::MockJson { status, body }) => {
                    let body = serde_json::to_vec(&body).unwrap_or_else(|_| b"null".to_vec());
                    execute_page_command(
                        &page,
                        "Fetch.fulfillRequest",
                        serde_json::json!({
                            "requestId": request_id,
                            "responseCode": status,
                            "responseHeaders": [
                                { "name": "Content-Type", "value": "application/json; charset=utf-8" },
                                { "name": "Cache-Control", "value": "no-store" }
                            ],
                            "body": BASE64.encode(body)
                        }),
                    )
                    .await
                }
                None => {
                    execute_page_command(
                        &page,
                        "Fetch.continueRequest",
                        serde_json::json!({ "requestId": request_id }),
                    )
                    .await
                }
            };
            if let Err(error) = result {
                tracing::warn!(%error, "failed to resolve an intercepted browser request");
            }
        }
    }))
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut position = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(offset) = value[position..].find(part) else {
            return false;
        };
        if index == 0 && !pattern.starts_with('*') && offset != 0 {
            return false;
        }
        position += offset + part.len();
    }
    pattern.ends_with('*') || parts.last().is_none_or(|part| value.ends_with(part))
}

async fn page_descriptor(page: &Page) -> TargetDescriptor {
    TargetDescriptor {
        id: page.target_id().as_ref().to_owned(),
        title: page.get_title().await.ok().flatten(),
        url: page.url().await.ok().flatten(),
        kind: "page".into(),
    }
}

#[derive(Debug)]
struct RawCommand {
    method: String,
    params: Value,
}

impl serde::Serialize for RawCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.params.serialize(serializer)
    }
}

impl Method for RawCommand {
    fn identifier(&self) -> MethodId {
        Cow::Owned(self.method.clone())
    }
}

impl Command for RawCommand {
    type Response = Value;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_patterns_are_anchored_when_no_outer_star_is_present() {
        assert!(wildcard_match("*/api/*", "https://example.com/api/data"));
        assert!(wildcard_match(
            "https://example.com/*",
            "https://example.com/a"
        ));
        assert!(!wildcard_match(
            "https://example.com/*",
            "xhttps://example.com/a"
        ));
        assert!(!wildcard_match(
            "*/api/data",
            "https://example.com/api/data/extra"
        ));
    }

    #[tokio::test]
    async fn event_queue_polls_by_monotonic_sequence() {
        let queue = BoundedEventQueue::default();
        queue
            .push(
                "Runtime.consoleAPICalled".into(),
                serde_json::json!({"value": 1}),
            )
            .await;
        queue
            .push(
                "Runtime.consoleAPICalled".into(),
                serde_json::json!({"value": 2}),
            )
            .await;
        let first = queue.poll(0, 1, Duration::ZERO).await;
        assert_eq!(first.events.len(), 1);
        assert_eq!(first.events[0].sequence, 1);
        let second = queue.poll(1, 10, Duration::ZERO).await;
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].sequence, 2);
        assert_eq!(second.latest_sequence, 2);
    }
}
