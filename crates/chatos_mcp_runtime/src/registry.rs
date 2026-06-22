use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::types::{ToolCallContext, ToolStreamChunkCallback};

#[async_trait]
pub trait BuiltinToolProvider: Send + Sync {
    fn server_name(&self) -> &str;

    fn list_tools(&self) -> Vec<Value>;

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String>;

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}

#[derive(Clone, Default)]
pub struct BuiltinToolRegistry {
    providers: HashMap<String, Arc<dyn BuiltinToolProvider>>,
}

impl BuiltinToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<P>(&mut self, provider: P)
    where
        P: BuiltinToolProvider + 'static,
    {
        self.providers
            .insert(provider.server_name().to_string(), Arc::new(provider));
    }

    pub fn register_arc(&mut self, provider: Arc<dyn BuiltinToolProvider>) {
        self.providers
            .insert(provider.server_name().to_string(), provider);
    }

    pub fn get(&self, server_name: &str) -> Option<Arc<dyn BuiltinToolProvider>> {
        self.providers.get(server_name).cloned()
    }

    pub fn contains(&self, server_name: &str) -> bool {
        self.providers.contains_key(server_name)
    }
}
