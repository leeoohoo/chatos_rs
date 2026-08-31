use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const SERVER_NAME: &str = "chatos-browser-cdp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserMode {
    Managed,
    ChromeExtension,
}

impl Default for BrowserMode {
    fn default() -> Self {
        Self::Managed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenBrowserRequest {
    #[serde(default)]
    pub mode: BrowserMode,
    #[serde(default = "default_true")]
    pub headless: bool,
    #[serde(default)]
    pub persistent_profile: bool,
    #[serde(default)]
    pub session_name: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserDescriptor {
    pub mode: BrowserMode,
    pub product: String,
    pub user_agent: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetDescriptor {
    pub id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendSessionId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub session_id: Option<BackendSessionId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpEvent {
    pub sequence: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatch {
    pub events: Vec<CdpEvent>,
    pub dropped_event_count: u64,
    pub latest_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RouteAction {
    Abort,
    MockJson {
        #[serde(default = "default_mock_status")]
        status: u16,
        body: Value,
    },
}

fn default_mock_status() -> u16 {
    200
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRule {
    pub url_pattern: String,
    pub action: RouteAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDescriptor {
    pub route_id: String,
    pub tab_id: String,
    pub rule: RouteRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    pub artifact_id: String,
    pub relative_path: String,
    pub display_name: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
}
