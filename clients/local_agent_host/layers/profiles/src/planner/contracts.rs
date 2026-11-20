/// A model runtime resolved for one step. This type is deliberately neither
/// serializable nor debuggable because `model_config` may contain credentials.
pub struct TransientLocalModelRuntime {
    pub runner: Arc<ContextualTurnRunner>,
    pub model_config: ModelRuntimeConfig,
}

#[async_trait]
pub trait LocalModelRuntimeResolver: Send + Sync {
    async fn resolve_model_runtime(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<TransientLocalModelRuntime, String>;
}

#[async_trait]
impl<T> LocalModelRuntimeResolver for Arc<T>
where
    T: LocalModelRuntimeResolver + ?Sized,
{
    async fn resolve_model_runtime(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<TransientLocalModelRuntime, String> {
        (**self)
            .resolve_model_runtime(owner_user_id, model_config_ref, model_config_revision)
            .await
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedLocalCapabilities {
    pub instructions: Option<String>,
    pub prefixed_input_items: Vec<Value>,
    pub tools: Vec<Value>,
}

#[async_trait]
pub trait LocalCapabilityResolver: Send + Sync {
    async fn resolve_capabilities(
        &self,
        owner_user_id: &str,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<ResolvedLocalCapabilities, String>;
}

#[async_trait]
impl<T> LocalCapabilityResolver for Arc<T>
where
    T: LocalCapabilityResolver + ?Sized,
{
    async fn resolve_capabilities(
        &self,
        owner_user_id: &str,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<ResolvedLocalCapabilities, String> {
        (**self)
            .resolve_capabilities(owner_user_id, profile_key, capability_policy_revision)
            .await
    }
}

#[async_trait]
pub trait LocalHookContextProvider: Send + Sync {
    async fn prefixed_context(&self, claim: &LocalAgentRunClaim) -> Result<Vec<Value>, String>;
}

#[async_trait]
impl<T> LocalHookContextProvider for Arc<T>
where
    T: LocalHookContextProvider + ?Sized,
{
    async fn prefixed_context(&self, claim: &LocalAgentRunClaim) -> Result<Vec<Value>, String> {
        (**self).prefixed_context(claim).await
    }
}
