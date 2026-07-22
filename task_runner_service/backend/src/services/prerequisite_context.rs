// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::BuiltinMcpPromptLocale;
use serde_json::{json, Value};

use crate::models::{TaskRecord, TaskRunRecord, TaskStatus};

use super::TaskStatusExt;

#[derive(Debug, Clone)]
pub(super) struct PrerequisiteTaskContext {
    pub(super) task_id: String,
    pub(super) title: String,
    pub(super) objective: String,
    pub(super) status: TaskStatus,
    pub(super) run_id: Option<String>,
    pub(super) result_summary: Option<String>,
    pub(super) run_result_summary: Option<String>,
    pub(super) process_log: Option<String>,
    pub(super) report_content: Option<String>,
}

pub(super) fn build_task_prompt(
    task: &TaskRecord,
    prompt_override: Option<&str>,
    prerequisite_context: &[PrerequisiteTaskContext],
    locale: BuiltinMcpPromptLocale,
) -> String {
    let text = task_prompt_text(locale);
    let mut current_task_prompt = if let Some(prompt) = prompt_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.to_string()
    } else {
        let mut parts = vec![
            format!("{}:\n{}", text.task_title_label, task.title),
            format!("{}:\n{}", text.task_objective_label, task.objective),
        ];
        if let Some(description) = task
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("{}:\n{description}", text.task_description_label));
        }
        if let Some(input_payload) = &task.input_payload {
            let payload_text = serde_json::to_string_pretty(input_payload)
                .unwrap_or_else(|_| input_payload.to_string());
            parts.push(format!("{}:\n{payload_text}", text.input_data_label));
        }
        parts.join("\n\n")
    };
    current_task_prompt.push_str("\n\n");
    current_task_prompt.push_str(task_output_language_policy(locale));
    if crate::models::uses_task_runner_planning_agent(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    ) {
        current_task_prompt.push_str(if locale.is_english() {
            "\n\nExecution policy: this is a pure planning task. Produce analysis, plans, task decomposition, or Project Management updates only. Do not run commands and do not create, modify, move, or delete project files."
        } else {
            "\n\n执行策略：这是纯规划任务。只产出分析、方案、任务拆分或 Project Management 更新；不得运行命令，不得创建、修改、移动或删除项目文件。"
        });
    } else if !task.mcp_config.requires_execution {
        current_task_prompt.push_str(if locale.is_english() {
            "\n\nExecution policy: this is a file-only task. Use the default sandbox to inspect and modify project files. Do not require, initialize, start, build, test, or validate the project's dedicated runtime environment unless the user explicitly changes the task policy."
        } else {
            "\n\n执行策略：这是一个仅文件处理任务。使用默认沙箱读取和修改项目文件；除非用户明确修改任务策略，否则不要要求、初始化、启动、构建、测试或验证项目专属运行环境。"
        });
    }

    if prerequisite_context.is_empty() {
        return current_task_prompt;
    }

    format!(
        "{}\n\n{}:\n\n{}",
        format_prerequisite_context_for_prompt(prerequisite_context, locale),
        text.current_task_heading,
        current_task_prompt
    )
}

pub(super) fn build_task_prompt_template(locale: BuiltinMcpPromptLocale) -> String {
    let text = task_prompt_text(locale);
    format!(
        "{}\n\n{}\n\n{}:\n\n{}:\n{{{{task.title}}}}\n\n{}:\n{{{{task.objective}}}}\n\n{}:\n{{{{task.description}}}}\n\n{}:\n{{{{task.input_payload_json}}}}",
        task_output_language_policy(locale),
        format_prerequisite_context_template(locale),
        text.current_task_heading,
        text.task_title_label,
        text.task_objective_label,
        text.task_description_label,
        text.input_data_label
    )
}

fn task_output_language_policy(locale: BuiltinMcpPromptLocale) -> &'static str {
    if locale.is_english() {
        "[Output Language Policy]\nUse the language explicitly requested by the user or used in the current task title/objective for progress notes, Project Management artifacts, result summaries, reports, and other user-visible prose. If the task text is mixed or contains no clear natural language, use English (en-US). Preserve code identifiers, commands, paths, API/library/product names, and quoted source text in their original form. Keep each newly written artifact internally consistent instead of mixing English and Chinese sentences."
    } else {
        "[输出语言规则]\n进度说明、Project Management 产物、结果摘要、报告及其他用户可见文本，应优先使用用户明确指定的语言，或当前任务标题/目标所使用的自然语言。任务文本语言混合或无法判断时，使用简体中文（zh-CN）。代码标识符、命令、路径、API、库/产品名和引用原文保持不变。每个新写入的产物应保持语言一致，不要混用中英文完整句子。"
    }
}

pub(super) fn build_global_execution_prompt(locale: BuiltinMcpPromptLocale) -> String {
    if locale.is_english() {
        "Managed by the published task_runner_run_phase Prompt in Plugin Management.".to_string()
    } else {
        "由 Plugin Management 中已发布的 task_runner_run_phase Prompt 统一管理。".to_string()
    }
}

fn format_prerequisite_context_for_prompt(
    contexts: &[PrerequisiteTaskContext],
    locale: BuiltinMcpPromptLocale,
) -> String {
    let text = task_prompt_text(locale);
    let mut parts = vec![format!("{}:", text.prerequisite_heading)];
    for (index, context) in contexts.iter().enumerate() {
        let mut item = vec![
            format!(
                "{}. [{}] {} / {}",
                index + 1,
                context.status.status_string(),
                context.task_id,
                context.title
            ),
            format!(
                "{}:\n{}",
                text.prerequisite_objective_label, context.objective
            ),
        ];
        if let Some(run_id) = context.run_id.as_deref() {
            item.push(format!("{}:\n{run_id}", text.latest_successful_run_label));
        }
        if let Some(summary) = context
            .run_result_summary
            .as_deref()
            .or(context.result_summary.as_deref())
        {
            item.push(format!("{}:\n{}", text.result_summary_label, summary));
        }
        if let Some(process_log) = context.process_log.as_deref() {
            item.push(format!(
                "{}:\n{}",
                text.execution_process_label, process_log
            ));
        }
        if let Some(content) = context.report_content.as_deref() {
            item.push(format!("{}:\n{}", text.key_output_label, content));
        }
        parts.push(item.join("\n"));
    }
    parts.join("\n\n")
}

fn format_prerequisite_context_template(locale: BuiltinMcpPromptLocale) -> String {
    let text = task_prompt_text(locale);
    [
        format!("{}:", text.prerequisite_heading),
        "1. [{{prerequisite.status}}] {{prerequisite.task_id}} / {{prerequisite.title}}"
            .to_string(),
        format!(
            "{}:\n{{{{prerequisite.objective}}}}",
            text.prerequisite_objective_label
        ),
        format!(
            "{}:\n{{{{prerequisite.run_id}}}}",
            text.latest_successful_run_label
        ),
        format!(
            "{}:\n{{{{prerequisite.result_summary}}}}",
            text.result_summary_label
        ),
        format!(
            "{}:\n{{{{prerequisite.process_log}}}}",
            text.execution_process_label
        ),
        format!("{}:\n{{{{prerequisite.report}}}}", text.key_output_label),
    ]
    .join("\n")
}

struct TaskPromptText {
    task_title_label: &'static str,
    task_objective_label: &'static str,
    task_description_label: &'static str,
    input_data_label: &'static str,
    prerequisite_heading: &'static str,
    current_task_heading: &'static str,
    prerequisite_objective_label: &'static str,
    latest_successful_run_label: &'static str,
    result_summary_label: &'static str,
    execution_process_label: &'static str,
    key_output_label: &'static str,
}

fn task_prompt_text(locale: BuiltinMcpPromptLocale) -> TaskPromptText {
    if locale.is_english() {
        TaskPromptText {
            task_title_label: "Task Title",
            task_objective_label: "Task Objective",
            task_description_label: "Task Description",
            input_data_label: "Input Data",
            prerequisite_heading: "Prerequisite Task Results",
            current_task_heading: "Current Task",
            prerequisite_objective_label: "Objective",
            latest_successful_run_label: "Latest Successful Run",
            result_summary_label: "Result Summary",
            execution_process_label: "Execution Process",
            key_output_label: "Key Output",
        }
    } else {
        TaskPromptText {
            task_title_label: "任务标题",
            task_objective_label: "任务目标",
            task_description_label: "任务说明",
            input_data_label: "输入数据",
            prerequisite_heading: "前置任务执行结果",
            current_task_heading: "当前任务",
            prerequisite_objective_label: "目标",
            latest_successful_run_label: "最近成功运行",
            result_summary_label: "结果摘要",
            execution_process_label: "执行过程",
            key_output_label: "关键输出",
        }
    }
}

pub(super) fn build_prerequisite_context(
    task: &TaskRecord,
    run: Option<&TaskRunRecord>,
) -> PrerequisiteTaskContext {
    PrerequisiteTaskContext {
        task_id: task.id.clone(),
        title: task.title.clone(),
        objective: task.objective.clone(),
        status: task.status,
        run_id: run.map(|run| run.id.clone()),
        result_summary: task.result_summary.clone(),
        run_result_summary: run.and_then(|run| run.result_summary.clone()),
        process_log: task.process_log.clone(),
        report_content: run.and_then(extract_report_content),
    }
}

pub(super) fn extract_report_content(run: &TaskRunRecord) -> Option<String> {
    run.report
        .as_ref()
        .and_then(|report| report.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn prerequisite_context_json(contexts: &[PrerequisiteTaskContext]) -> Value {
    json!(contexts
        .iter()
        .map(|context| {
            json!({
                "task_id": context.task_id,
                "title": context.title,
                "objective": context.objective,
                "status": context.status.status_string(),
                "run_id": context.run_id,
                "result_summary": context.result_summary,
                "run_result_summary": context.run_result_summary,
                "process_log": context.process_log,
                "report_content": context.report_content,
            })
        })
        .collect::<Vec<_>>())
}

pub(super) fn attach_prerequisite_context_to_run(
    run: &mut TaskRunRecord,
    contexts: &[PrerequisiteTaskContext],
) {
    let context_json = prerequisite_context_json(contexts);
    if let Some(object) = run.input_snapshot.as_object_mut() {
        object.insert("resolved_prerequisites".to_string(), context_json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_prompt_template_keeps_user_language_policy_in_both_locales() {
        let chinese = build_task_prompt_template(BuiltinMcpPromptLocale::ZhCn);
        assert!(chinese.contains("输出语言规则"));
        assert!(chinese.contains("当前任务标题/目标所使用的自然语言"));
        assert!(chinese.contains("Project Management 产物"));

        let english = build_task_prompt_template(BuiltinMcpPromptLocale::EnUs);
        assert!(english.contains("Output Language Policy"));
        assert!(english.contains("current task title/objective"));
        assert!(english.contains("Project Management artifacts"));
    }
}
