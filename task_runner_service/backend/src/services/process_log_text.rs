use crate::models::{RecordTaskProcessRequest, TaskProcessLogOperation};

use super::{TASK_PROCESS_LOG_MAX_CHARS, normalized_optional};

pub(super) fn apply_task_process_log_update(
    current: Option<String>,
    input: RecordTaskProcessRequest,
    now: &str,
) -> Result<Option<String>, String> {
    match input.operation {
        TaskProcessLogOperation::Clear => Ok(None),
        TaskProcessLogOperation::Replace => {
            let content = normalized_optional(input.content);
            validate_process_log_length(content.as_deref())?;
            Ok(content)
        }
        TaskProcessLogOperation::Append => {
            let content =
                normalized_optional(input.content).ok_or_else(|| "content 不能为空".to_string())?;
            let entry = format_task_process_entry(now, input.heading, content);
            let next = match normalized_optional(current) {
                Some(existing) => format!("{existing}\n\n{entry}"),
                None => entry,
            };
            validate_process_log_length(Some(next.as_str()))?;
            Ok(Some(next))
        }
    }
}

fn format_task_process_entry(now: &str, heading: Option<String>, content: String) -> String {
    let heading = normalized_optional(heading);
    match heading {
        Some(heading) => format!("[{now}] {heading}\n{content}"),
        None => format!("[{now}]\n{content}"),
    }
}

fn validate_process_log_length(value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let len = value.chars().count();
    if len > TASK_PROCESS_LOG_MAX_CHARS {
        Err(format!(
            "过程记录不能超过 {TASK_PROCESS_LOG_MAX_CHARS} 字符，当前 {len} 字符"
        ))
    } else {
        Ok(())
    }
}
