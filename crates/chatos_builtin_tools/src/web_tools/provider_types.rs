// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct BrowserRenderOptions {
    pub workspace_dir: PathBuf,
    pub command_timeout_seconds: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub url: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractedPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub content_chars: usize,
    pub original_content_chars: usize,
    pub truncated: bool,
    pub content_summary: Option<ExtractContentSummary>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractSummaryChunk {
    pub index: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub preview: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractContentSummary {
    pub strategy: String,
    pub total_chars: usize,
    pub chunk_chars: usize,
    pub total_chunks: usize,
    pub sampled_chunks: Vec<ExtractSummaryChunk>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderAttempt {
    pub provider: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchOutcome {
    pub backend: String,
    pub fallback_used: bool,
    pub attempts: Vec<ProviderAttempt>,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractOutcome {
    pub backend: String,
    pub fallback_used: bool,
    pub attempts: Vec<ProviderAttempt>,
    pub pages: Vec<ExtractedPage>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResearchUrlCandidate {
    pub url: String,
    pub host: String,
    pub article_like: bool,
    pub section_like: bool,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct HtmlMetadata {
    pub title: String,
    pub description: String,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct StructuredDataContent {
    pub title: String,
    pub description: String,
    pub text_blocks: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct BrowserRenderedPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub meta_description: String,
    pub snapshot: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ResponseContentKind {
    Html,
    Json,
    Text,
    Pdf,
    Unsupported(String),
}
