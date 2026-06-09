use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ResearchSummaryView {
    pub page_success: Option<bool>,
    pub search_count: u64,
    pub extract_count: u64,
    pub selected_url_count: u64,
    pub truncated_page_count: u64,
    pub total_omitted_chars: u64,
    pub extract_backend: String,
    pub warning_present: bool,
}

pub fn research_summary_view(response: &Value) -> ResearchSummaryView {
    let summary = response
        .get("research_summary")
        .and_then(|value| value.as_object());

    ResearchSummaryView {
        page_success: summary
            .and_then(|value| value.get("page_success"))
            .and_then(|value| value.as_bool()),
        search_count: summary
            .and_then(|value| value.get("search_result_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        extract_count: summary
            .and_then(|value| value.get("extracted_page_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        selected_url_count: summary
            .and_then(|value| value.get("selected_url_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        truncated_page_count: summary
            .and_then(|value| value.get("truncated_page_count"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        total_omitted_chars: summary
            .and_then(|value| value.get("total_omitted_chars"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        extract_backend: summary
            .and_then(|value| value.get("extract_backend"))
            .and_then(|value| value.as_str())
            .unwrap_or("none")
            .to_string(),
        warning_present: summary
            .and_then(|value| value.get("warning"))
            .and_then(|value| value.as_str())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
    }
}
