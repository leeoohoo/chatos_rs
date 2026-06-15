use super::*;

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct ModelConfigIdArgs {
    pub(in crate::mcp_server) model_config_id: String,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct UpdateModelConfigArgs {
    pub(in crate::mcp_server) model_config_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) patch: UpdateModelConfigRequest,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct TestModelConfigArgs {
    pub(in crate::mcp_server) model_config_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) prompt: Option<String>,
}
