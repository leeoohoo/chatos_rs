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
    let current_task_prompt = if let Some(prompt) = prompt_override
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
        "{}\n\n{}:\n\n{}:\n{{{{task.title}}}}\n\n{}:\n{{{{task.objective}}}}\n\n{}:\n{{{{task.description}}}}\n\n{}:\n{{{{task.input_payload_json}}}}\n\n{}",
        format_prerequisite_context_template(locale),
        text.current_task_heading,
        text.task_title_label,
        text.task_objective_label,
        text.task_description_label,
        text.input_data_label,
        text.completion_instruction
    )
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
            completion_instruction: "Start executing directly based on the task objective. If tools are available, call them when needed. Finish with a clear result, key findings, and recommended next steps.",
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
            completion_instruction: "请根据任务目标直接开始执行；如果有可用工具，请在必要时调用；最终给出清晰的结果、关键发现和后续建议。",
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
