impl LocalAgentHostCoordinator {
    pub async fn run_until_shutdown(
        &self,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        if let Some(hooks) = self.hook_coordinator.as_ref() {
            let _ = hooks
                .abandon_interrupted_async_invocations(&self.owner_user_id)
                .await;
            let _ = hooks.recover_pending_events(&self.owner_user_id).await;
        }
        let work_shutdown = shutdown.clone();
        let work = async move {
            if self.memory_sync_worker.is_none() {
                self.run_schedulers_until_shutdown(work_shutdown).await
            } else {
                let scheduler_loop = self.run_schedulers_until_shutdown(work_shutdown.clone());
                let memory_loop = self.run_memory_until_shutdown(work_shutdown);
                let (scheduler_result, memory_result) = tokio::join!(scheduler_loop, memory_loop);
                scheduler_result.and(memory_result)
            }
        };
        let hook_sessions = self.run_hook_session_reaper_until_shutdown(shutdown);
        let (work_result, hook_result) = tokio::join!(work, hook_sessions);
        let result = work_result.and(hook_result);
        if let Some(hooks) = self.hook_coordinator.as_ref() {
            let _ = hooks
                .enqueue_session_end(&self.owner_user_id, "shutdown")
                .await;
        }
        result
    }

    async fn run_hook_session_reaper_until_shutdown(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let mut interval = tokio::time::interval(HOOK_SESSION_REAPER_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
                _ = interval.tick() => {
                    if self.work_activity.active_count() == 0 {
                        if let Some(hooks) = self.hook_coordinator.as_ref() {
                            let _ = hooks.expire_idle_sessions(
                                &self.owner_user_id,
                                Instant::now(),
                                HOOK_SESSION_IDLE_TIMEOUT,
                            ).await;
                        }
                    }
                }
            }
        }
    }

    async fn run_schedulers_until_shutdown(
        &self,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let workers = (0..self.maximum_concurrent_work_items).map(|worker_index| {
            self.run_scheduler_worker_until_shutdown(worker_index, shutdown.clone())
        });
        for result in join_all(workers).await {
            result?;
        }
        Ok(())
    }

    /// Keeps one global work slot alive for the lifetime of the Host.
    ///
    /// A batch-oriented loop cannot reuse slots that found no work until the
    /// slowest future in that batch returns. A Run arriving in the meantime is
    /// then queued behind unrelated work despite available capacity. Persistent
    /// workers refill their slots independently and preserve the configured
    /// global concurrency limit across model and tool work.
    async fn run_scheduler_worker_until_shutdown(
        &self,
        worker_index: usize,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let mut effective_limit = self.effective_maximum_concurrent_work_items.subscribe();
        let mut accepting_new_work = self.accepting_new_work.subscribe();
        let mut failure_backoff = Duration::from_millis(100);
        // Split the initial preference across workers, then rotate it after every committed
        // item. Model steps and Tool calls share one machine budget, but neither lane may own
        // every slot merely because it is always checked first. This is especially important
        // when several slow model requests are active while an earlier Run is already waiting
        // for a Tool result.
        let mut prefer_tool = worker_index % 2 == 1;
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            while !*accepting_new_work.borrow() {
                tokio::select! {
                    changed = accepting_new_work.changed() => {
                        if changed.is_err() {
                            return Ok(());
                        }
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            return Ok(());
                        }
                    }
                }
            }
            while worker_index >= *effective_limit.borrow() {
                tokio::select! {
                    changed = effective_limit.changed() => {
                        if changed.is_err() {
                            return Ok(());
                        }
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            return Ok(());
                        }
                    }
                }
            }
            let progressed = match self
                .run_ready_work_once(worker_index, prefer_tool, &shutdown)
                .await
            {
                Ok(progressed) => {
                    failure_backoff = Duration::from_millis(100);
                    progressed
                }
                Err(_) => {
                    tokio::select! {
                        _ = self.wakeup.notified() => {}
                        _ = tokio::time::sleep(failure_backoff) => {}
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() {
                                return Ok(());
                            }
                        }
                    }
                    failure_backoff = (failure_backoff * 2).min(Duration::from_secs(5));
                    continue;
                }
            };
            if *shutdown.borrow() {
                return Ok(());
            }
            if progressed {
                prefer_tool = !prefer_tool;
                continue;
            }
            let delay = match self.next_run_retry_delay().await {
                Ok(delay) => delay,
                Err(_) => Duration::from_secs(1),
            };
            tokio::select! {
                _ = self.wakeup.notified() => {}
                _ = tokio::time::sleep(delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn run_ready_work_once(
        &self,
        worker_index: usize,
        prefer_tool: bool,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<bool, LocalAgentCoordinatorError> {
        let first = if prefer_tool {
            self.run_tool_once().await
        } else {
            self.run_model_once().await
        };
        match first {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(first_error) => {
                // A broken or temporarily unavailable lane must not suppress independent work in
                // the other lane. Preserve the first error only if the fallback also cannot make
                // progress, allowing the outer loop to apply its bounded failure backoff.
                if !self.worker_can_claim(worker_index, shutdown) {
                    return Err(first_error);
                }
                let fallback = if prefer_tool {
                    self.run_model_once().await
                } else {
                    self.run_tool_once().await
                };
                return match fallback {
                    Ok(true) => Ok(true),
                    Ok(false) | Err(_) => Err(first_error),
                };
            }
        }
        if !self.worker_can_claim(worker_index, shutdown) {
            return Ok(false);
        }
        if prefer_tool {
            self.run_model_once().await
        } else {
            self.run_tool_once().await
        }
    }

    fn worker_can_claim(&self, worker_index: usize, shutdown: &watch::Receiver<bool>) -> bool {
        !*shutdown.borrow()
            && *self.accepting_new_work.borrow()
            && worker_index < *self.effective_maximum_concurrent_work_items.borrow()
    }

    async fn run_model_once(&self) -> Result<bool, LocalAgentCoordinatorError> {
        let Some(scheduler) = self.model_scheduler.as_ref() else {
            return Ok(false);
        };
        let Some(_activity) = self.begin_admitted_work(&self.admission_activity).await else {
            return Ok(false);
        };
        let committed = matches!(scheduler.run_once().await?, SchedulerTick::Committed(_));
        if committed {
            self.signal_activity();
        }
        Ok(committed)
    }

    async fn run_tool_once(&self) -> Result<bool, LocalAgentCoordinatorError> {
        let Some(scheduler) = self.tool_scheduler.as_ref() else {
            return Ok(false);
        };
        let Some(_activity) = self.begin_admitted_work(&self.admission_activity).await else {
            return Ok(false);
        };
        let committed = matches!(scheduler.run_once().await?, ToolSchedulerTick::Committed(_));
        if committed {
            self.signal_activity();
        }
        Ok(committed)
    }

    async fn begin_admitted_work(
        &self,
        activity: &HostWorkActivity,
    ) -> Option<super::work_activity::HostWorkActivityGuard> {
        // Admission changes take the write side of this barrier. Once a close command has
        // returned, every worker that passed the gate has already published its activity guard,
        // and every other worker observes the closed gate. This removes the health-snapshot race
        // where a worker could claim immediately after the client observed an idle Host.
        let barrier = self.admission_barrier.read().await;
        if !*self.accepting_new_work.borrow() {
            return None;
        }
        let guard = activity.begin();
        drop(barrier);
        Some(guard)
    }

    #[cfg(test)]
    async fn drain_ready_work(
        &self,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let mut round = 0usize;
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            let results = join_all((0..self.maximum_concurrent_work_items).map(|worker_index| {
                self.run_ready_work_once(worker_index, (worker_index + round) % 2 == 1, shutdown)
            }))
            .await;
            let mut progressed = false;
            for result in results {
                progressed |= result?;
            }
            if !progressed {
                return Ok(());
            }
            round = round.wrapping_add(1);
        }
    }

    async fn next_run_retry_delay(&self) -> Result<Duration, LocalAgentCoordinatorError> {
        let next_retry_at = self.runtime.next_retry_at(&self.owner_user_id).await?;
        let Some(next_retry_at) = next_retry_at else {
            return Ok(RUN_IDLE_RECOVERY_INTERVAL);
        };
        let now = system_now_unix_ms()?;
        Ok(Duration::from_millis(
            u64::try_from(next_retry_at.saturating_sub(now).max(0)).unwrap_or(0),
        )
        .min(RUN_IDLE_RECOVERY_INTERVAL))
    }
}
