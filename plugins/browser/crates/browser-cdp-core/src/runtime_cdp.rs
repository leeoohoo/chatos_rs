use super::*;

impl BrowserRuntime {
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
}
