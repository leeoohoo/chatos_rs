impl LocalAgentHostCoordinator {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
        model_scheduler: Option<LocalAgentScheduler>,
        tool_scheduler: Option<LocalToolScheduler>,
    ) -> Result<Self, String> {
        if model_scheduler.is_none() && tool_scheduler.is_none() {
            return Err("Local Agent coordinator requires a model or tool scheduler".to_string());
        }
        let owner_user_id = owner_user_id.into();
        validate_identifier("owner_user_id", &owner_user_id)?;
        if model_scheduler
            .as_ref()
            .is_some_and(|scheduler| scheduler.owner_user_id() != owner_user_id)
            || tool_scheduler
                .as_ref()
                .is_some_and(|scheduler| scheduler.owner_user_id() != owner_user_id)
        {
            return Err(
                "Local Agent coordinator and schedulers must use the same owner".to_string(),
            );
        }
        let (activity, _) = watch::channel(0);
        let maximum_concurrent_work_items = default_maximum_concurrent_work_items();
        let (target_concurrent_work_items, _) = watch::channel(maximum_concurrent_work_items);
        let (effective_maximum_concurrent_work_items, _) =
            watch::channel(maximum_concurrent_work_items);
        let (pressure_level, _) = watch::channel(LocalAgentResourcePressureLevel::Normal);
        let (accepting_new_work, _) = watch::channel(true);
        let work_activity = HostWorkActivity::default();
        Ok(Self {
            runtime,
            owner_user_id,
            model_scheduler: model_scheduler
                .map(|scheduler| scheduler.with_work_activity(work_activity.clone())),
            tool_scheduler: tool_scheduler
                .map(|scheduler| scheduler.with_work_activity(work_activity.clone())),
            memory_sync_worker: None,
            hook_coordinator: None,
            wakeup: Arc::new(Notify::new()),
            activity,
            work_activity,
            admission_activity: HostWorkActivity::default(),
            memory_work_activity: HostWorkActivity::default(),
            reserved_ipc_tools: Vec::new(),
            maximum_concurrent_work_items,
            target_concurrent_work_items,
            effective_maximum_concurrent_work_items,
            pressure_level,
            accepting_new_work,
            admission_barrier: Arc::new(RwLock::new(())),
        })
    }

    pub fn with_maximum_concurrent_work_items(mut self, maximum: usize) -> Result<Self, String> {
        if !(MIN_CONCURRENT_WORK_ITEMS..=MAX_CONCURRENT_WORK_ITEMS).contains(&maximum) {
            return Err(format!(
                "maximum concurrent work items must be between {MIN_CONCURRENT_WORK_ITEMS} and {MAX_CONCURRENT_WORK_ITEMS}"
            ));
        }
        self.maximum_concurrent_work_items = maximum;
        self.target_concurrent_work_items.send_replace(maximum);
        self.effective_maximum_concurrent_work_items
            .send_replace(maximum);
        Ok(self)
    }

    pub fn with_target_concurrent_work_items(self, target: usize) -> Result<Self, String> {
        if !(MIN_CONCURRENT_WORK_ITEMS..=self.maximum_concurrent_work_items).contains(&target) {
            return Err(format!(
                "target concurrent work items must be between {MIN_CONCURRENT_WORK_ITEMS} and {}",
                self.maximum_concurrent_work_items
            ));
        }
        self.target_concurrent_work_items.send_replace(target);
        self.effective_maximum_concurrent_work_items
            .send_replace(target);
        Ok(self)
    }

    pub fn with_memory_sync_worker(
        mut self,
        worker: LocalMemorySyncWorker,
    ) -> Result<Self, String> {
        if worker.tenant_id() != self.owner_user_id {
            return Err(
                "Local Agent coordinator and Memory worker must use the same owner".to_string(),
            );
        }
        self.memory_sync_worker = Some(worker);
        Ok(self)
    }

    pub fn with_hook_coordinator(mut self, coordinator: Arc<LocalHookCoordinator>) -> Self {
        self.tool_scheduler = self
            .tool_scheduler
            .map(|scheduler| scheduler.with_hook_coordinator(Arc::clone(&coordinator)));
        self.hook_coordinator = Some(coordinator);
        self
    }

    pub fn with_reserved_ipc_tools<I, S>(mut self, tool_names: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut names = Vec::new();
        for tool_name in tool_names {
            let tool_name = tool_name.into();
            validate_identifier("reserved_ipc_tool", &tool_name)?;
            if names.contains(&tool_name) {
                return Err(format!("reserved IPC tool is duplicated: {tool_name}"));
            }
            names.push(tool_name);
        }
        if names.len() > 128 {
            return Err("at most 128 IPC tools can be reserved".to_string());
        }
        self.reserved_ipc_tools = names;
        Ok(self)
    }

    pub fn wake(&self) {
        // One command can make several durable work items ready. Wake every
        // idle slot so capacity is filled immediately, and retain one permit
        // for the case where all slots are currently busy.
        self.wakeup.notify_waiters();
        self.wakeup.notify_one();
    }
}
