use super::*;

pub(crate) fn todo_planning_read_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "todo_execution_options",
            "description": "仅供项目经理读取当前管理团队、可分配成员、基础能力和本机 Plugin 临时选项。真实团队、项目、Agent 和 Plugin ID 不会返回。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_dependency_options",
            "description": "仅供项目经理读取当前团队可作为前置任务的 Todo 临时引用。只能建立同团队依赖。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600}
                },
                "required": ["team_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_list",
            "description": "读取当前 Agent 所属项目团队的共享任务板，并标明负责人、依赖、执行能力和是否分配给自己。默认不返回已完成或已取消任务。",
            "parameters": {
                "type": "object",
                "properties": {
                    "include_terminal": {"type": "boolean", "default": false}
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_read_progress",
            "description": "通讯线程读取一个 Todo 执行线程持续写入的阶段、动作、结果或阻塞记录。使用 todo_list 返回的临时 todo_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "todo_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 100}
                },
                "required": ["todo_ref"],
                "additionalProperties": false
            }
        }),
    ]
}

pub(crate) fn todo_planning_write_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "todo_add",
            "description": "仅供项目经理在当前共享团队任务板创建 Todo。必须先调用 todo_execution_options，并明确目标、范围、交付物、验收条件、负责人、前置任务和可信执行能力。来源消息由客户端绑定为当前 Run 的触发消息，真实 ID 不会暴露。",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": 500},
                    "detail": {"type": "string", "maxLength": 16000},
                    "objective": {"type": "string", "minLength": 1, "maxLength": 8000},
                    "scope": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "expected_outputs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "acceptance_criteria": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "constraints": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "maxItems": 64, "uniqueItems": true},
                    "priority": {"type": "integer", "minimum": 0, "maximum": 100, "default": 50},
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "assignee_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "depends_on_todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "maxItems": 64, "uniqueItems": true},
                    "requires_execution": {"type": "boolean", "default": true},
                    "builtin_capabilities": {"type": "array", "items": {"type": "string", "enum": ["project_read", "project_write", "terminal", "requirement_survey_read", "requirement_survey_write"]}, "maxItems": 5, "uniqueItems": true},
                    "plugin_hints": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "plugin_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                                "reason": {"type": "string", "maxLength": 1000}
                            },
                            "required": ["plugin_ref"],
                            "additionalProperties": false
                        },
                        "maxItems": 32
                    }
                },
                "required": ["title", "objective", "scope", "expected_outputs", "acceptance_criteria", "team_ref", "assignee_ref", "requires_execution", "builtin_capabilities"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_update",
            "description": "仅供项目经理更新待处理或普通阻塞 Todo 的内容、优先级、前置任务，或将阻塞任务重开、取消过时任务。completed 和 blocked 执行终态由 executor 工具写入；正在执行的 Todo 必须通过持久化 Run 控制停止后才能编辑。",
            "parameters": {
                "type": "object",
                "properties": {
                    "todo_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "title": {"type": "string", "minLength": 1, "maxLength": 500},
                    "detail": {"type": "string", "maxLength": 16000},
                    "objective": {"type": "string", "minLength": 1, "maxLength": 8000},
                    "scope": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "expected_outputs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "acceptance_criteria": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "minItems": 1, "maxItems": 64, "uniqueItems": true},
                    "constraints": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 4000}, "maxItems": 64, "uniqueItems": true},
                    "priority": {"type": "integer", "minimum": 0, "maximum": 100},
                    "status": {"type": "string", "enum": ["pending", "cancelled"]},
                    "depends_on_todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "maxItems": 64, "uniqueItems": true}
                },
                "required": ["todo_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_reorder",
            "description": "仅供项目经理显式调整当前团队任务板中未完成 Todo 的优先顺序。数组中靠前的任务优先；未完成前置任务仍不会被调度。",
            "parameters": {
                "type": "object",
                "properties": {
                    "todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "minItems": 1, "maxItems": 500, "uniqueItems": true}
                },
                "required": ["todo_refs"],
                "additionalProperties": false
            }
        }),
    ]
}

pub(crate) fn todo_execution_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "todo_get_context",
            "description": "仅用于 Todo 执行线程：读取当前 delivery 绑定的任务、可信能力计划、前置任务、团队资产摘要和最近进度，不接受任何 ID 参数。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_progress_append",
            "description": "仅用于 Todo 执行线程：记录当前负责人的阶段、动作、观察结果和下一步。",
            "parameters": {
                "type": "object",
                "properties": {
                    "stage": {"type": "string", "maxLength": 240},
                    "detail": {"type": "string", "minLength": 1, "maxLength": 16000}
                },
                "required": ["detail"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_complete",
            "description": "仅用于 Todo 执行线程：保存当前负责人可核验的完成总结并结束任务。project_write 任务必须先完成事务提交和提交后读取。",
            "parameters": {
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "asset_update_suggestions": {
                        "type": "array",
                        "maxItems": 8,
                        "items": {
                            "type": "object",
                            "properties": {
                                "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                                "title": {"type": "string", "minLength": 1, "maxLength": 240},
                                "markdown": {"type": "string", "minLength": 1, "maxLength": 128000},
                                "rationale": {"type": "string", "minLength": 1, "maxLength": 4000}
                            },
                            "required": ["category", "title", "markdown", "rationale"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["summary"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "todo_block",
            "description": "仅用于 Todo 执行线程：因缺少必要信息、权限、访问或安全恢复路径而无法继续时，保存精确原因并阻塞任务。",
            "parameters": {
                "type": "object",
                "properties": {
                    "reason": {"type": "string", "minLength": 1, "maxLength": 8000}
                },
                "required": ["reason"],
                "additionalProperties": false
            }
        }),
    ]
}
