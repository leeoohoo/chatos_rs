pub(super) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn default_project_name(project_id: &str) -> String {
    if project_id.trim() == "0" {
        "未指定项目".to_string()
    } else {
        format!("项目 {}", project_id.trim())
    }
}
