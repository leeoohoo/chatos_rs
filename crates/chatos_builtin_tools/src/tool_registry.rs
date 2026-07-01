// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use serde_json::{json, Value};

#[derive(Clone)]
pub struct RegisteredTool<H> {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: H,
}

#[derive(Clone)]
pub struct ToolRegistry<H> {
    tools: HashMap<String, RegisteredTool<H>>,
    unavailable_tools: HashMap<String, String>,
}

impl<H> ToolRegistry<H> {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            unavailable_tools: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&RegisteredTool<H>> {
        self.tools.get(name)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect()
    }

    pub fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: H,
    ) {
        self.tools.insert(
            name.to_string(),
            RegisteredTool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
    }

    pub fn register_unavailable_tool(&mut self, name: &str, reason: String) {
        self.unavailable_tools.insert(name.to_string(), reason);
    }

    pub fn register_unavailable_tools<'a, I>(&mut self, names: I, reason: String)
    where
        I: IntoIterator<Item = &'a str>,
    {
        for name in names {
            self.register_unavailable_tool(name, reason.clone());
        }
    }

    pub fn unavailable_tools(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .unavailable_tools
            .iter()
            .map(|(name, reason)| (name.clone(), reason.clone()))
            .collect();
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        pairs
    }
}

pub fn async_text_tool_handler<F, Fut>(
    builder: F,
) -> Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>
where
    F: Fn(Value) -> Result<Fut, String> + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, String>>,
{
    Arc::new(move |args| {
        let future = builder(args)?;
        let result = block_on_result(future)?;
        Ok(text_result(result))
    })
}

pub(crate) fn block_on_result<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())?;
        runtime.block_on(future)
    }
}

pub(crate) fn block_on_option<F, T>(future: F) -> Option<T>
where
    F: Future<Output = Option<T>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        runtime.block_on(future)
    }
}

pub fn text_result(payload: Value) -> Value {
    let text = if payload.is_string() {
        payload.as_str().unwrap_or("").to_string()
    } else if let Some(summary) = payload
        .get("_summary_text")
        .and_then(|value| value.as_str())
    {
        summary.to_string()
    } else {
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
    };

    let mut out = json!({
        "content": [
            { "type": "text", "text": text }
        ]
    });
    if !payload.is_string() && !payload.is_null() {
        out["_structured_result"] = payload;
    }
    out
}
