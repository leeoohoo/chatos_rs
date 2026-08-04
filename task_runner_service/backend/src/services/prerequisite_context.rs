// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::BuiltinMcpPromptLocale;
use serde_json::{json, Value};

use crate::models::{TaskRecord, TaskRunRecord, TaskStatus};

use super::TaskStatusExt;

const PREREQUISITE_PROCESS_LOG_MAX_CHARS: usize = 4_000;

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
    retry_instruction: Option<&str>,
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
            "\n\nPlanning scope: use project facts as read-only evidence and deliver analysis, technical plans, implementation tasks, and dependency updates. Engineering changes and runtime validation follow in the execution stage."
        } else {
            "\n\n规划范围：以只读方式了解项目事实，交付分析、技术方案、实施任务和依赖关系；工程修改与运行验证由后续执行阶段承接。"
        });
    } else if !task.mcp_config.requires_execution {
        current_task_prompt.push_str(if locale.is_english() {
            "\n\nExecution policy: this is a file-only task. Use the default sandbox to inspect and modify project files. Do not require, initialize, start, build, test, or validate the project's dedicated runtime environment unless the user explicitly changes the task policy."
        } else {
            "\n\n执行策略：这是一个仅文件处理任务。使用默认沙箱读取和修改项目文件；除非用户明确修改任务策略，否则不要要求、初始化、启动、构建、测试或验证项目专属运行环境。"
        });
    }
    append_retry_instruction(&mut current_task_prompt, retry_instruction, locale);

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

fn append_retry_instruction(
    prompt: &mut String,
    retry_instruction: Option<&str>,
    locale: BuiltinMcpPromptLocale,
) {
    let Some(instruction) = retry_instruction
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    prompt.push_str(if locale.is_english() {
        "\n\n[User guidance for this retry]\n"
    } else {
        "\n\n[用户对本次重试的补充处理意见]\n"
    });
    prompt.push_str(instruction);
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
        "[Output Language Policy]\nUse the language explicitly requested by the user or used in the current task title/objective for progress notes, project artifacts, result summaries, reports, and other user-visible prose. If the task text is mixed or contains no clear natural language, use English (en-US). Preserve code identifiers, commands, paths, API/library/product names, and quoted source text in their original form. Keep each newly written artifact internally consistent instead of mixing English and Chinese sentences."
    } else {
        "[输出语言规则]\n进度说明、项目资料、结果摘要、报告及其他用户可见文本，应优先使用用户明确指定的语言，或当前任务标题/目标所使用的自然语言。任务文本语言混合或无法判断时，使用简体中文（zh-CN）。代码标识符、命令、路径、API、库/产品名和引用原文保持不变。每个新写入的产物应保持语言一致，不要混用中英文完整句子。"
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
    let run_result_summary = run.and_then(|run| run.result_summary.clone());
    let report_content = run.and_then(extract_report_content);
    let has_terminal_output =
        run_result_summary.is_some() || task.result_summary.is_some() || report_content.is_some();
    PrerequisiteTaskContext {
        task_id: task.id.clone(),
        title: task.title.clone(),
        objective: task.objective.clone(),
        status: task.status,
        run_id: run.map(|run| run.id.clone()),
        result_summary: task.result_summary.clone(),
        run_result_summary,
        process_log: prerequisite_process_log_for_context(
            task.process_log.as_deref(),
            has_terminal_output,
        ),
        report_content,
    }
}

fn prerequisite_process_log_for_context(
    process_log: Option<&str>,
    has_terminal_output: bool,
) -> Option<String> {
    if has_terminal_output {
        return None;
    }
    bounded_prerequisite_process_log(process_log)
}

fn bounded_prerequisite_process_log(process_log: Option<&str>) -> Option<String> {
    let process_log = process_log
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let total_chars = process_log.chars().count();
    if total_chars <= PREREQUISITE_PROCESS_LOG_MAX_CHARS {
        return Some(process_log.to_string());
    }
    let tail = process_log
        .chars()
        .skip(total_chars - PREREQUISITE_PROCESS_LOG_MAX_CHARS)
        .collect::<String>();
    Some(format!(
        "[较早的执行过程已省略，仅保留最近 {PREREQUISITE_PROCESS_LOG_MAX_CHARS} 字符]\n{tail}"
    ))
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
        assert!(chinese.contains("项目资料"));

        let english = build_task_prompt_template(BuiltinMcpPromptLocale::EnUs);
        assert!(english.contains("Output Language Policy"));
        assert!(english.contains("current task title/objective"));
        assert!(english.contains("project artifacts"));
    }

    #[test]
    fn retry_instruction_is_appended_without_replacing_the_task_prompt() {
        let mut prompt = "任务标题：修复阻塞节点".to_string();
        append_retry_instruction(
            &mut prompt,
            Some("  配置已经补齐，请重新验证  "),
            BuiltinMcpPromptLocale::ZhCn,
        );

        assert!(prompt.starts_with("任务标题：修复阻塞节点"));
        assert!(prompt.contains("[用户对本次重试的补充处理意见]"));
        assert!(prompt.ends_with("配置已经补齐，请重新验证"));
    }

    #[test]
    fn prerequisite_process_log_keeps_short_content_unchanged() {
        assert_eq!(
            bounded_prerequisite_process_log(Some("最近一次执行已完成")),
            Some("最近一次执行已完成".to_string())
        );
    }

    #[test]
    fn prerequisite_process_log_drops_stale_prefix_and_keeps_bounded_tail() {
        let stale_prefix = "旧运行反复分析未完成".repeat(1_000);
        let recent_result = "最近运行已经完成真实改动和测试";
        let log = format!("{stale_prefix}\n{recent_result}");

        let bounded = bounded_prerequisite_process_log(Some(log.as_str()))
            .expect("bounded prerequisite process log");

        assert!(bounded.starts_with("[较早的执行过程已省略"));
        assert!(bounded.ends_with(recent_result));
        assert!(!bounded.contains(stale_prefix.as_str()));
        assert!(bounded.chars().count() < PREREQUISITE_PROCESS_LOG_MAX_CHARS + 100);
    }

    #[test]
    fn prerequisite_terminal_output_takes_precedence_over_process_log() {
        assert_eq!(
            prerequisite_process_log_for_context(Some("旧运行未完成"), true),
            None
        );
    }
}
