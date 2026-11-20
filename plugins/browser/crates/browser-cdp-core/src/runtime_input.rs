use super::*;

impl BrowserRuntime {
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
                self.session(browser_session_id)
                    .await?
                    .lock()
                    .await
                    .invalidate_refs();
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
                        let changed =
                            page.url != before.url || page.document_id != before.document_id;
                        let ready = page.ready_state == "complete";
                        after = Some(page);
                        if (changed && ready) || Instant::now() >= deadline {
                            break;
                        }
                    } else if Instant::now() >= deadline {
                        break;
                    }
                }
                if let Some(page) = &after {
                    let session = self.session(browser_session_id).await?;
                    let mut session = session.lock().await;
                    if let Some(tab) = session
                        .tabs
                        .values_mut()
                        .find(|tab| tab.backend_session_id.0 == backend_session_id.0)
                    {
                        tab.url = Some(page.url.clone());
                        tab.title = Some(page.title.clone());
                    }
                }
                let navigated = after
                    .as_ref()
                    .map(|page| page.url != before.url || page.document_id != before.document_id);
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
        let should_move_mouse = should_dispatch_mouse_move(mode, &point);

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

        self.session(browser_session_id)
            .await?
            .lock()
            .await
            .invalidate_refs();
        let mut timed_out_events = Vec::new();
        let mut skipped_events = Vec::new();
        if !should_move_mouse {
            // chrome.debugger executes background-tab mouseMoved commands but
            // can leave their response Promise pending forever. mousePressed
            // carries the same coordinates, so avoid creating a leaked Bridge
            // request when no user can observe the background hover animation.
            skipped_events.push("mouseMoved:background_tab");
        } else if !dispatch_mouse_event(
            backend.as_ref(),
            &backend_session_id,
            json!({
                "type": "mouseMoved",
                "x": x,
                "y": y,
                "button": "none",
                "buttons": 0,
                "pointerType": "mouse"
            }),
        )
        .await?
        {
            timed_out_events.push("mouseMoved");
        }
        if !dispatch_mouse_event(
            backend.as_ref(),
            &backend_session_id,
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "buttons": 1,
                "clickCount": 1,
                "pointerType": "mouse"
            }),
        )
        .await?
        {
            timed_out_events.push("mousePressed");
        }
        let _ = evaluate_value(
            backend.as_ref(),
            &backend_session_id,
            &virtual_cursor_pulse_script(),
        )
        .await;
        // Keep the pressed cursor visible before mouseReleased triggers a
        // navigation and destroys the current page overlay.
        tokio::time::sleep(Duration::from_millis(VIRTUAL_CURSOR_CLICK_HOLD_MS)).await;
        if !dispatch_mouse_event(
            backend.as_ref(),
            &backend_session_id,
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "buttons": 0,
                "clickCount": 1,
                "pointerType": "mouse"
            }),
        )
        .await?
        {
            timed_out_events.push("mouseReleased");
        }
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
                if (changed && ready) || Instant::now() >= deadline {
                    break;
                }
            } else if Instant::now() >= deadline {
                break;
            }
        }
        if let Some(page) = &after {
            let session = self.session(browser_session_id).await?;
            let mut session = session.lock().await;
            if let Some(tab) = session
                .tabs
                .values_mut()
                .find(|tab| tab.backend_session_id.0 == backend_session_id.0)
            {
                tab.url = Some(page.url.clone());
                tab.title = Some(page.title.clone());
            }
        }
        let navigated = after
            .as_ref()
            .map(|page| page.url != before.url || page.document_id != before.document_id);
        let dispatch_response = if timed_out_events.is_empty() {
            "confirmed"
        } else {
            "timeout_recovered"
        };
        Ok(
            json!({"clicked": true, "before": before, "after": after, "navigated": navigated,
            "dispatch_response": dispatch_response, "timed_out_events": timed_out_events,
            "skipped_events": skipped_events,
            "refs_invalidated": true, "next_action": "Take a fresh browser_snapshot; if the page is still loading, wait and snapshot again. Do not repeat a consequential click just because observation is pending."}),
        )
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
