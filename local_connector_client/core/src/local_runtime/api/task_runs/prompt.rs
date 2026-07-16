// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::local_runtime::project_management::{
    LocalRequirementDocumentRecord, LocalRequirementRecord, LocalWorkItemRecord,
};

pub(super) fn task_run_prompt(
    requirement: &LocalRequirementRecord,
    work_item: &LocalWorkItemRecord,
    documents: &[LocalRequirementDocumentRecord],
) -> String {
    let documents = documents
        .iter()
        .map(|document| {
            format!(
                "### {}\n{}",
                document.title,
                truncate(document.content.as_str(), 6_000)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        r#"execution_location: local_connector_client
cloud_services_allowed: false

需求：{requirement_title}
需求摘要：{requirement_summary}
需求详情：{requirement_detail}
验收标准：{acceptance_criteria}

当前任务：{task_title}
任务说明：{task_description}

相关技术文档：
{documents}"#,
        requirement_title = requirement.title,
        requirement_summary = requirement.summary.as_deref().unwrap_or(""),
        requirement_detail = requirement.detail.as_deref().unwrap_or(""),
        acceptance_criteria = requirement.acceptance_criteria.as_deref().unwrap_or(""),
        task_title = work_item.title,
        task_description = work_item.description.as_deref().unwrap_or(""),
        documents = if documents.is_empty() {
            "无"
        } else {
            documents.as_str()
        },
    )
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        format!(
            "{}\n[truncated]",
            value.chars().take(limit).collect::<String>()
        )
    }
}
