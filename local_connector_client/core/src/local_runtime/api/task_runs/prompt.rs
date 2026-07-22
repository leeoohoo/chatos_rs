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
    let task_nature = if work_item.is_planning_task {
        "规划任务：只产出分析、方案或项目管理结果，不执行命令，不创建或修改项目文件。"
    } else {
        "实施任务：必须直接完成真实代码、文件、命令和验证工作。需求或技术文档中类似“当前仅做规划”“本轮不修改文件”“暂不运行命令”的文字属于创建规划时的历史回合约束，不适用于本次已明确触发的实施执行。"
    };
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
任务性质：{task_nature}

相关技术文档：
{documents}"#,
        requirement_title = requirement.title,
        requirement_summary = requirement.summary.as_deref().unwrap_or(""),
        requirement_detail = requirement.detail.as_deref().unwrap_or(""),
        acceptance_criteria = requirement.acceptance_criteria.as_deref().unwrap_or(""),
        task_title = work_item.title,
        task_description = work_item.description.as_deref().unwrap_or(""),
        task_nature = task_nature,
        documents = if documents.is_empty() {
            "无"
        } else {
            documents.as_str()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::task_run_prompt;
    use crate::local_runtime::project_management::{LocalRequirementRecord, LocalWorkItemRecord};

    fn requirement() -> LocalRequirementRecord {
        LocalRequirementRecord {
            id: "requirement-1".to_string(),
            project_id: "project-1".to_string(),
            parent_requirement_id: None,
            requirement_type: "requirement".to_string(),
            title: "需求".to_string(),
            summary: Some("摘要".to_string()),
            detail: Some("当前仅做规划，不修改文件。".to_string()),
            business_value: None,
            acceptance_criteria: Some("完成实现".to_string()),
            source: None,
            priority: 0,
            status: "approved".to_string(),
            creator_user_id: Some("user-1".to_string()),
            owner_user_id: Some("user-1".to_string()),
            assignee_user_id: None,
            created_at: "2026-07-19T00:00:00Z".to_string(),
            updated_at: "2026-07-19T00:00:00Z".to_string(),
            archived_at: None,
        }
    }

    fn work_item(is_planning_task: bool) -> LocalWorkItemRecord {
        LocalWorkItemRecord {
            id: "task-1".to_string(),
            project_id: "project-1".to_string(),
            requirement_id: "requirement-1".to_string(),
            title: "任务".to_string(),
            description: Some("执行任务".to_string()),
            status: "todo".to_string(),
            priority: 0,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: Vec::new(),
            is_planning_task,
            creator_user_id: Some("user-1".to_string()),
            owner_user_id: Some("user-1".to_string()),
            created_at: "2026-07-19T00:00:00Z".to_string(),
            updated_at: "2026-07-19T00:00:00Z".to_string(),
            archived_at: None,
        }
    }

    #[test]
    fn implementation_task_overrides_historical_plan_only_text() {
        let prompt = task_run_prompt(&requirement(), &work_item(false), &[]);
        assert!(prompt.contains("实施任务：必须直接完成真实代码"));
        assert!(prompt.contains("不适用于本次已明确触发的实施执行"));
    }

    #[test]
    fn planning_task_keeps_non_mutating_boundary() {
        let prompt = task_run_prompt(&requirement(), &work_item(true), &[]);
        assert!(prompt.contains("规划任务：只产出分析"));
        assert!(prompt.contains("不创建或修改项目文件"));
    }
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
