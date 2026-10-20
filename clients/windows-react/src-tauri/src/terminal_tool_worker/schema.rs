use serde_json::{json, Value};

const MINIMUM_FOREGROUND_TIMEOUT_MS: u64 = 1_000;
const MAXIMUM_FOREGROUND_TIMEOUT_MS: u64 = 15 * 60 * 1_000;
const MAXIMUM_WAIT_MS: u64 = 2 * 60 * 60 * 1_000;
const MAXIMUM_PURPOSE_CHARACTERS: u64 = 500;

pub(crate) fn terminal_tool_schemas() -> Vec<Value> {
    vec![
        function(
            "execute_command",
            "在当前项目中执行 command 指定的完整 shell 命令。purpose 必须用面向用户的自然语言说明本次运行目的，供审批界面优先展示；不要在 purpose 中重复粘贴命令。前台默认最多等待 300 秒；长时间运行的命令应使用 background=true，再通过 process_poll 或 process_wait 跟踪。",
            json!({
                "path": {"type": "string"},
                "purpose": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAXIMUM_PURPOSE_CHARACTERS,
                    "description": "向用户说明为什么需要运行这条命令，以及预期得到什么结果。使用简短自然语言，不要复制 shell 命令。"
                },
                "command": {
                    "type": "string",
                    "minLength": 1,
                    "description": "要实际执行的完整 shell 命令或脚本正文；不要填写自然语言摘要。"
                },
                "background": {"type": "boolean", "default": false},
                "timeout_ms": integer(MINIMUM_FOREGROUND_TIMEOUT_MS, MAXIMUM_FOREGROUND_TIMEOUT_MS),
                "timeout": integer(
                    MINIMUM_FOREGROUND_TIMEOUT_MS / 1_000,
                    MAXIMUM_FOREGROUND_TIMEOUT_MS / 1_000
                )
            }),
            &["purpose", "command"],
        ),
        function(
            "process_poll",
            "轮询命令进程的状态和增量日志。",
            process_read_properties(80),
            &["terminal_id"],
        ),
        function(
            "process_log",
            "分页读取命令进程日志。",
            process_read_properties(200),
            &["terminal_id"],
        ),
        function(
            "process_wait",
            "等待命令进程结束或等待超时。",
            json!({
                "terminal_id": {"type": "string"},
                "timeout_ms": integer(1_000, MAXIMUM_WAIT_MS),
                "timeout": integer(1, MAXIMUM_WAIT_MS / 1_000)
            }),
            &["terminal_id"],
        ),
        function(
            "process_write",
            "向命令进程写入标准输入。",
            json!({
                "terminal_id": {"type": "string"},
                "data": {"type": "string"},
                "submit": {"type": "boolean", "default": false}
            }),
            &["terminal_id", "data"],
        ),
        function(
            "process_kill",
            "终止命令进程。",
            json!({"terminal_id": {"type": "string"}}),
            &["terminal_id"],
        ),
    ]
}

fn process_read_properties(default_limit: u64) -> Value {
    json!({
        "terminal_id": {"type": "string"},
        "offset": {"type": "integer", "minimum": 0},
        "limit": {
            "type": "integer",
            "minimum": 1,
            "maximum": 200,
            "default": default_limit
        }
    })
}

fn integer(minimum: u64, maximum: u64) -> Value {
    json!({
        "type": "integer",
        "minimum": minimum,
        "maximum": maximum
    })
}

fn function(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn publishes_only_the_six_macos_task_terminal_tools() {
        let schemas = terminal_tool_schemas();
        let names = schemas
            .iter()
            .filter_map(|schema| schema.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            names,
            BTreeSet::from([
                "execute_command",
                "process_kill",
                "process_log",
                "process_poll",
                "process_wait",
                "process_write",
            ])
        );
        assert!(schemas.iter().all(|schema| {
            schema.pointer("/parameters/additionalProperties") == Some(&Value::Bool(false))
        }));
    }

    #[test]
    fn command_and_wait_bounds_match_the_macos_catalog() {
        let schemas = terminal_tool_schemas();
        let command = schema(&schemas, "execute_command");
        assert_eq!(
            command.pointer("/parameters/properties/purpose/maxLength"),
            Some(&json!(500))
        );
        assert_eq!(
            command.pointer("/parameters/properties/timeout_ms/maximum"),
            Some(&json!(900_000))
        );
        let wait = schema(&schemas, "process_wait");
        assert_eq!(
            wait.pointer("/parameters/properties/timeout_ms/maximum"),
            Some(&json!(7_200_000))
        );
    }

    fn schema<'a>(schemas: &'a [Value], name: &str) -> &'a Value {
        schemas
            .iter()
            .find(|schema| schema.get("name") == Some(&Value::String(name.into())))
            .expect("terminal schema")
    }
}
