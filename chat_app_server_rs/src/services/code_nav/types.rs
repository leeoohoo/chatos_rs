use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub file_path: PathBuf,
    pub relative_path: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavPositionRequest {
    pub project_root: String,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSymbolsRequest {
    pub project_root: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavCapabilities {
    pub supports_definition: bool,
    pub supports_references: bool,
    pub supports_document_symbols: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavCapabilitiesResponse {
    pub language: String,
    pub provider: String,
    pub supports_definition: bool,
    pub supports_references: bool,
    pub supports_document_symbols: bool,
    pub fallback_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavLocation {
    pub path: String,
    pub relative_path: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub preview: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavLocationsResponse {
    pub provider: String,
    pub language: String,
    pub mode: String,
    pub token: Option<String>,
    pub locations: Vec<NavLocation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentSymbolItem {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentSymbolsResponse {
    pub provider: String,
    pub language: String,
    pub mode: String,
    pub symbols: Vec<DocumentSymbolItem>,
}
