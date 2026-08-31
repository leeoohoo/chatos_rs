use std::{path::Path, sync::Arc, time::Duration};

use async_trait::async_trait;
use browser_cdp_protocol::{
    BackendSessionId, BrowserDescriptor, BrowserMode, EventBatch, EventFilter, OpenBrowserRequest,
    RouteRule, TargetDescriptor,
};
use serde_json::Value;

use crate::CoreResult;

#[async_trait]
pub trait BrowserBackend: Send + Sync {
    async fn open(&self, request: OpenBrowserRequest) -> CoreResult<BrowserDescriptor>;
    async fn list_targets(&self) -> CoreResult<Vec<TargetDescriptor>>;
    async fn create_target(&self, url: &str) -> CoreResult<TargetDescriptor>;
    async fn close_target(&self, target_id: &str) -> CoreResult<()>;
    async fn attach_target(&self, target_id: &str) -> CoreResult<BackendSessionId>;
    async fn detach_target(&self, session_id: &BackendSessionId) -> CoreResult<()>;
    async fn send_command(
        &self,
        session_id: Option<&BackendSessionId>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> CoreResult<Value>;
    async fn subscribe(&self, _filter: EventFilter) -> CoreResult<String> {
        Err(crate::CoreError::Unsupported(
            "event subscriptions are not implemented by this backend".into(),
        ))
    }
    async fn poll_events(
        &self,
        _subscription_id: &str,
        _after_sequence: u64,
        _max_events: usize,
        _wait: Duration,
    ) -> CoreResult<EventBatch> {
        Err(crate::CoreError::Unsupported(
            "event polling is not implemented by this backend".into(),
        ))
    }
    async fn unsubscribe(&self, _subscription_id: &str) -> CoreResult<()> {
        Err(crate::CoreError::Unsupported(
            "event subscriptions are not implemented by this backend".into(),
        ))
    }
    async fn add_route(
        &self,
        _session_id: &BackendSessionId,
        _rule: RouteRule,
    ) -> CoreResult<String> {
        Err(crate::CoreError::Unsupported(
            "request routing is not implemented by this backend".into(),
        ))
    }
    async fn remove_route(&self, _route_id: &str) -> CoreResult<()> {
        Err(crate::CoreError::Unsupported(
            "request routing is not implemented by this backend".into(),
        ))
    }
    async fn configure_downloads(&self, _download_dir: &Path) -> CoreResult<()> {
        Err(crate::CoreError::Unsupported(
            "downloads are not implemented by this backend".into(),
        ))
    }
    async fn disable_downloads(&self) -> CoreResult<()> {
        Err(crate::CoreError::Unsupported(
            "downloads are not implemented by this backend".into(),
        ))
    }
    async fn close(&self) -> CoreResult<()>;
}

#[async_trait]
pub trait BrowserBackendFactory: Send + Sync {
    fn supports(&self, mode: BrowserMode) -> bool;
    async fn create(&self, mode: BrowserMode) -> CoreResult<Arc<dyn BrowserBackend>>;
}
