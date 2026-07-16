// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::scan::LocalProjectScanEvidence;

pub(super) fn environment_analysis_prompt(
    project_id: &str,
    project_name: &str,
    evidence: &LocalProjectScanEvidence,
    capability_prompt: Option<&str>,
) -> Result<String, String> {
    let evidence = serde_json::to_string_pretty(evidence).map_err(|error| error.to_string())?;
    Ok(format!(
        r#"你是本地 Project Environment Agent。所有编排发生在 Local Connector 客户端；禁止建议调用云端项目服务。

根据下面由 Rust 在本机扫描得到的证据，为项目生成运行环境计划。不要修改源码，不要执行命令，不要臆造未检测到的依赖。

project_id: {project_id}
project_name: {project_name}

本地扫描证据：
{evidence}

只输出一个 JSON 对象，不要 Markdown。结构必须是：
{{
  "status": "ready|not_runnable|pending_configuration",
  "analysis_summary": "中文摘要",
  "not_runnable_reason": null,
  "detected_stack": {{}},
  "required_services": [],
  "environment_variables": {{}},
  "generated_config_files": [],
  "images": [{{
    "environment_key": "app",
    "environment_type": "application|service",
    "display_name": "名称",
    "image_ref": null,
    "features": [],
    "ports": [],
    "env_vars": {{}}
  }}]
}}

规则：应用运行时使用 environment_type=application；数据库、缓存、消息队列等使用 service。计划状态由客户端保存为 planned。只有确实没有可识别运行入口时才返回 not_runnable。{}"#,
        optional_capability_prompt(capability_prompt),
    ))
}

fn optional_capability_prompt(prompt: Option<&str>) -> String {
    prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("\n\n插件能力约束：\n{value}"))
        .unwrap_or_default()
}

pub(super) fn normalize_analysis(
    mut value: super::LocalEnvironmentAnalysisResult,
) -> Result<super::LocalEnvironmentAnalysisResult, String> {
    value.status = value.status.trim().to_ascii_lowercase();
    if !matches!(
        value.status.as_str(),
        "ready" | "not_runnable" | "pending_configuration"
    ) {
        return Err(format!("unsupported environment status: {}", value.status));
    }
    if value.analysis_summary.trim().is_empty() {
        return Err("environment analysis_summary is required".to_string());
    }
    value.detected_stack = object_or_default(value.detected_stack);
    value.required_services = array_or_default(value.required_services);
    value.env_vars = object_or_default(value.env_vars);
    value.generated_config_files = array_or_default(value.generated_config_files);
    value
        .images
        .retain(|image| !image.environment_key.trim().is_empty());
    Ok(value)
}

fn object_or_default(value: Value) -> Value {
    value
        .is_object()
        .then_some(value)
        .unwrap_or_else(|| serde_json::json!({}))
}

fn array_or_default(value: Value) -> Value {
    value
        .is_array()
        .then_some(value)
        .unwrap_or_else(|| serde_json::json!([]))
}
