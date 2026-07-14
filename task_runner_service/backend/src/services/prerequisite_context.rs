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
        parts.push(text.completion_instruction.to_string());
        parts.join("\n\n")
    };
    if !task.mcp_config.requires_execution {
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
        "{}\n\n{}:\n\n{}:\n{{{{task.title}}}}\n\n{}:\n{{{{task.objective}}}}\n\n{}:\n{{{{task.description}}}}\n\n{}:\n{{{{task.input_payload_json}}}}",
        format_prerequisite_context_template(locale),
        text.current_task_heading,
        text.task_title_label,
        text.task_objective_label,
        text.task_description_label,
        text.input_data_label
    )
}

pub(super) fn build_global_execution_prompt(locale: BuiltinMcpPromptLocale) -> String {
    task_prompt_text(locale).completion_instruction.to_string()
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
    completion_instruction: &'static str,
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
            completion_instruction: "Engineering rules for concrete work:\n- Understand the real flow before changing anything: read the relevant task context, prerequisite results, files, logs, callers, or tool state before choosing the fix.\n- Before adding code, config, scripts, prompts, pages, or docs, ask whether anything needs to be built at all; then reuse existing project helpers and patterns; then use the standard library or native platform; then use already-installed dependencies; only then write the minimum new code that works.\n- Prefer deletion, reuse, and the shortest correct diff over new abstractions, new dependencies, boilerplate, speculative configuration, or \"maybe later\" extensibility.\n- For bugs, fix the shared root cause, not only the reported symptom; check sibling callers or adjacent paths when the touched code is reused.\n- Do not remove trust-boundary validation, error handling that prevents data loss, security checks, accessibility basics, or explicitly requested behavior in the name of simplicity.\n- For non-trivial changes, leave the smallest useful verification evidence, such as a focused command, test, log check, or clear reason why verification could not be run.\n\nExecute the task directly. Use tools only when they provide needed facts, changes, or verification. Finish with the result, key evidence, verification performed or skipped, and any necessary next step.",
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
            completion_instruction: "具体工程工作规则：\n- 改任何东西前先理解真实链路：读取相关任务上下文、前置结果、文件、日志、调用方或工具状态后，再决定怎么做。\n- 新增代码、配置、脚本、prompt、页面或文档前，先判断是否真的需要新增；再优先复用项目已有 helper、模式和约定；再用标准库或平台原生能力；再用已安装依赖；最后才写最小可工作的新增实现。\n- 优先删除、复用和最短正确 diff，不要新增未请求的抽象、依赖、样板、投机配置或“以后可能用”的扩展层。\n- 修 bug 要修共享根因，不只补当前症状；如果触碰的是复用代码，要检查相邻调用方或同类路径。\n- 不要为了“简单”删掉信任边界校验、防止数据丢失的错误处理、安全检查、可访问性基础或用户明确要求的行为。\n- 非平凡改动要留下最小但有用的验证证据，例如聚焦的命令、测试、日志检查，或说明为什么无法验证。\n\n请直接执行当前任务。只有当工具能提供事实、修改或验证时才调用工具。最终输出结果、关键证据、已执行或跳过的验证，以及必要的下一步。",
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
