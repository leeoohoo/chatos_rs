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
