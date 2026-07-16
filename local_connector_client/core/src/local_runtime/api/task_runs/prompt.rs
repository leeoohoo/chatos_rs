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
        r#"你是本地 Task Runner Worker。请在当前 Local Connector 项目工作区完成下面的项目任务。

必须遵守：
- 所有模型、工具、Skill、MCP、Ask User 和数据写入都在客户端执行。
- 禁止调用云端 Task Runner、Project Management 或 Memory 服务。
- 先检查现有代码和依赖，再实施修改并验证。
- 使用 Task Manager 更新本轮执行计划；完成后给出结果、验证和剩余风险。

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
