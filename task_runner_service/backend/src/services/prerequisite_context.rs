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
) -> String {
    let current_task_prompt = if let Some(prompt) = prompt_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.to_string()
    } else {
        let mut parts = vec![
            format!("任务标题:\n{}", task.title),
            format!("任务目标:\n{}", task.objective),
        ];
        if let Some(description) = task
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("任务说明:\n{description}"));
        }
        if let Some(input_payload) = &task.input_payload {
            let payload_text = serde_json::to_string_pretty(input_payload)
                .unwrap_or_else(|_| input_payload.to_string());
            parts.push(format!("输入数据:\n{payload_text}"));
        }
        parts.push("请根据任务目标直接开始执行；如果有可用工具，请在必要时调用；最终给出清晰的结果、关键发现和后续建议。".to_string());
        parts.join("\n\n")
    };

    if prerequisite_context.is_empty() {
        return current_task_prompt;
    }

    format!(
        "{}\n\n当前任务:\n\n{}",
        format_prerequisite_context_for_prompt(prerequisite_context),
        current_task_prompt
    )
}

fn format_prerequisite_context_for_prompt(contexts: &[PrerequisiteTaskContext]) -> String {
    let mut parts = vec!["前置任务执行结果:".to_string()];
    for (index, context) in contexts.iter().enumerate() {
        let mut item = vec![
            format!(
                "{}. [{}] {} / {}",
                index + 1,
                context.status.status_string(),
                context.task_id,
                context.title
            ),
            format!("目标:\n{}", context.objective),
        ];
        if let Some(run_id) = context.run_id.as_deref() {
            item.push(format!("最近成功运行:\n{run_id}"));
        }
        if let Some(summary) = context
            .run_result_summary
            .as_deref()
            .or(context.result_summary.as_deref())
        {
            item.push(format!("结果摘要:\n{}", summary));
        }
        if let Some(process_log) = context.process_log.as_deref() {
            item.push(format!("执行过程:\n{}", process_log));
        }
        if let Some(content) = context.report_content.as_deref() {
            item.push(format!("关键输出:\n{}", content));
        }
        parts.push(item.join("\n"));
    }
    parts.join("\n\n")
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
