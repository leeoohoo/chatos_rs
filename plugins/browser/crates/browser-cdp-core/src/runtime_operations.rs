use super::*;

impl BrowserRuntime {
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

    pub(super) async fn session(
        &self,
        browser_session_id: &str,
    ) -> CoreResult<Arc<Mutex<BrowserSession>>> {
        self.sessions
            .read()
            .await
            .get(browser_session_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound(format!("browser session {browser_session_id}")))
    }

    pub(super) async fn resolve_ref(
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
