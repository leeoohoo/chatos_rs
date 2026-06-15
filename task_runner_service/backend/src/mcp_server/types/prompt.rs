use super::*;

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct PromptIdArgs {
    pub(in crate::mcp_server) prompt_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct ListPromptsArgs {
    #[serde(default)]
    pub(in crate::mcp_server) task_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) run_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) status: Option<UiPromptStatus>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct SubmitPromptArgs {
    pub(in crate::mcp_server) prompt_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) values: Option<Value>,
    #[serde(default)]
    pub(in crate::mcp_server) selection: Option<Value>,
    #[serde(default)]
    pub(in crate::mcp_server) reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct CancelPromptArgs {
    pub(in crate::mcp_server) prompt_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) reason: Option<String>,
}
