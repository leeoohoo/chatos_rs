pub struct ControlPlaneLocalAiStepPlanner {
    profile_key: &'static str,
    initial_text_field: &'static str,
    model_resolver: Arc<dyn LocalModelRuntimeResolver>,
    capability_resolver: Arc<dyn LocalCapabilityResolver>,
    memory_source_id: Option<String>,
    local_tools: Vec<Value>,
    local_tool_prefixes: Vec<String>,
    local_prefixed_input_items: Vec<Value>,
    local_context_runtime: Option<Arc<LocalAgentRuntime>>,
    hook_context_provider: Option<Arc<dyn LocalHookContextProvider>>,
}

impl ControlPlaneLocalAiStepPlanner {
    pub fn main_chat<M, C>(model_resolver: M, capability_resolver: C) -> Self
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
    {
        Self::new(
            MAIN_CHAT_PROFILE_KEY,
            "message",
            model_resolver,
            capability_resolver,
        )
    }

    pub fn task_execution<M, C>(model_resolver: M, capability_resolver: C) -> Self
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
    {
        Self::new(
            TASK_EXECUTION_PROFILE_KEY,
            "prompt",
            model_resolver,
            capability_resolver,
        )
    }

    fn new<M, C>(
        profile_key: &'static str,
        initial_text_field: &'static str,
        model_resolver: M,
        capability_resolver: C,
    ) -> Self
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
    {
        Self {
            profile_key,
            initial_text_field,
            model_resolver: Arc::new(model_resolver),
            capability_resolver: Arc::new(capability_resolver),
            memory_source_id: None,
            local_tools: Vec::new(),
            local_tool_prefixes: Vec::new(),
            local_prefixed_input_items: Vec::new(),
            local_context_runtime: None,
            hook_context_provider: None,
        }
    }

    /// Adds client-owned tools to every resolved capability revision. A local
    /// definition replaces a control-plane definition with the same name so
    /// retired server implementations cannot shadow the on-device executor.
    pub fn with_local_tools(mut self, tools: Vec<Value>) -> Result<Self, String> {
        validate_unique_tool_names(&tools)?;
        self.local_tools = tools;
        Ok(self)
    }

    /// Reserves a client-owned tool namespace. Control-plane definitions in
    /// that namespace are removed even when the local client intentionally
    /// supports only a smaller replacement surface.
    pub fn with_local_tool_prefixes<I, S>(mut self, prefixes: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.local_tool_prefixes = prefixes
            .into_iter()
            .map(Into::into)
            .map(|prefix: String| prefix.trim().to_string())
            .collect();
        if self
            .local_tool_prefixes
            .iter()
            .any(|prefix| prefix.is_empty())
        {
            return Err("local tool prefix must not be empty".to_string());
        }
        Ok(self)
    }

    pub fn with_local_prefixed_input_items(mut self, items: Vec<Value>) -> Self {
        self.local_prefixed_input_items = items;
        self
    }

    pub fn with_local_conversation_context(mut self, runtime: Arc<LocalAgentRuntime>) -> Self {
        self.local_context_runtime = Some(runtime);
        self
    }

    pub fn with_hook_context_provider<P>(mut self, provider: P) -> Self
    where
        P: LocalHookContextProvider + 'static,
    {
        self.hook_context_provider = Some(Arc::new(provider));
        self
    }

    pub fn with_memory_source_id(mut self, source_id: impl Into<String>) -> Result<Self, String> {
        let source_id = source_id.into().trim().to_string();
        if source_id.is_empty() {
            return Err("Memory source_id must not be empty".to_string());
        }
        self.memory_source_id = Some(source_id);
        Ok(self)
    }
}
