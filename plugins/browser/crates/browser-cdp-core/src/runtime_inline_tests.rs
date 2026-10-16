#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

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

    struct ObservedLinkBackend {
        page: StdMutex<PageState>,
        methods: StdMutex<Vec<String>>,
    }

    #[async_trait]
    impl BrowserBackend for ObservedLinkBackend {
        async fn open(&self, request: OpenBrowserRequest) -> CoreResult<BrowserDescriptor> {
            Ok(BrowserDescriptor {
                mode: request.mode,
                product: "paired-chrome".into(),
                user_agent: "test-agent".into(),
                capabilities: vec!["native_tab_groups".into()],
            })
        }

        async fn list_targets(&self) -> CoreResult<Vec<TargetDescriptor>> {
            Ok(vec![])
        }

        async fn create_target(&self, url: &str) -> CoreResult<TargetDescriptor> {
            Ok(TargetDescriptor {
                id: "task-tab".into(),
                title: Some("Start".into()),
                url: Some(url.into()),
                kind: "page".into(),
            })
        }

        async fn close_target(&self, _target_id: &str) -> CoreResult<()> { Ok(()) }

        async fn attach_target(&self, _target_id: &str) -> CoreResult<BackendSessionId> {
            Ok(BackendSessionId("task-session".into()))
        }

        async fn detach_target(&self, _session_id: &BackendSessionId) -> CoreResult<()> { Ok(()) }

        async fn send_command(
            &self,
            _session_id: Option<&BackendSessionId>,
            method: &str,
            params: Value,
            _timeout: Duration,
        ) -> CoreResult<Value> {
            self.methods.lock().unwrap().push(method.into());
            match method {
                "Runtime.evaluate" => {
                    let expression = params["expression"].as_str().unwrap_or_default();
                    let page = self.page.lock().unwrap().clone();
                    let value = if expression.contains("text_truncated") {
                        json!({
                            "url": page.url,
                            "title": page.title,
                            "ready_state": page.ready_state,
                            "document_id": page.document_id,
                            "text": "Official article",
                            "text_truncated": false,
                            "nodes": [{
                                "role": "link", "name": "MiniMax H3", "value": null,
                                "href": "https://www.minimax.io/news/h3", "tag": "a",
                                "selector": "body > a:nth-of-type(1)", "disabled": false
                            }]
                        })
                    } else {
                        serde_json::to_value(page).unwrap()
                    };
                    Ok(json!({"result": {"value": value}}))
                }
                "Page.navigate" => {
                    let mut page = self.page.lock().unwrap();
                    page.url = params["url"].as_str().unwrap().into();
                    page.title = "MiniMax H3".into();
                    page.ready_state = "complete".into();
                    page.document_id = "doc-2".into();
                    Ok(json!({"frameId": "frame-1"}))
                }
                other => Err(CoreError::Backend(format!("unexpected command: {other}"))),
            }
        }

        async fn close(&self) -> CoreResult<()> { Ok(()) }
    }

    struct ObservedLinkBackendFactory {
        backend: Arc<ObservedLinkBackend>,
    }

    #[async_trait]
    impl BrowserBackendFactory for ObservedLinkBackendFactory {
        fn supports(&self, mode: BrowserMode) -> bool { mode == BrowserMode::ChromeExtension }

        async fn create(&self, _mode: BrowserMode) -> CoreResult<Arc<dyn BrowserBackend>> {
            Ok(self.backend.clone())
        }
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

    #[tokio::test]
    async fn chrome_extension_click_follows_the_exact_safe_href_from_the_fresh_snapshot() {
        let backend = Arc::new(ObservedLinkBackend {
            page: StdMutex::new(PageState {
                url: "about:blank".into(),
                title: "Start".into(),
                ready_state: "complete".into(),
                document_id: "doc-1".into(),
            }),
            methods: StdMutex::new(Vec::new()),
        });
        let runtime = BrowserRuntime::new(
            vec![Arc::new(ObservedLinkBackendFactory { backend: backend.clone() })],
            std::env::temp_dir(),
        );
        let session = runtime.open_session(OpenBrowserRequest {
            mode: BrowserMode::ChromeExtension,
            headless: false,
            persistent_profile: false,
            session_name: Some("H3 verification".into()),
        }).await.expect("open paired Chrome session");
        let snapshot = runtime.snapshot(&session.browser_session_id, None)
            .await.expect("snapshot task tab");
        let link = snapshot.nodes.first().expect("observed link");

        let result = runtime.click(&session.browser_session_id, &link.reference)
            .await.expect("follow observed safe link");

        assert_eq!(result["interaction"], "follow_snapshot_link");
        assert_eq!(result["after"]["url"], "https://www.minimax.io/news/h3");
        assert_eq!(result["navigated"], true);
        let methods = backend.methods.lock().unwrap().clone();
        assert!(methods.contains(&"Page.navigate".to_string()));
        assert!(!methods.contains(&"Input.dispatchMouseEvent".to_string()));
        let stale = runtime.click(&session.browser_session_id, &link.reference).await;
        assert!(matches!(stale, Err(CoreError::InvalidRequest(_)) | Err(CoreError::NotFound(_))));
    }
}
