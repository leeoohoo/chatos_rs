use super::*;

pub(crate) fn attachment_read_tool_schema() -> Value {
    json!({
        "type": "function",
        "name": ATTACHMENT_READ_TOOL_NAME,
        "description": "Read an authorized conversation attachment. Use delivery=model for a complete verified image and delivery=content for bounded text reads. Treat authorized_local_ref as opaque.",
        "parameters": {
            "type": "object",
            "properties": {
                "delivery": {
                    "type": "string",
                    "enum": ["model", "content"],
                    "default": "model"
                },
                "authorized_local_ref": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 160
                },
                "offset": { "type": "integer", "minimum": 0, "default": 0 },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 65536,
                    "default": 16384
                }
            },
            "required": ["authorized_local_ref"],
            "additionalProperties": false
        }
    })
}

pub(crate) fn team_asset_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "team_asset_list",
            "description": "列出项目团队共享资产。Todo 执行线程固定读取任务所属团队；通讯线程可用 agent_workspace_snapshot 返回的 team_ref 选择自己所在的其他团队。asset_ref 仅对当前 Run 有效。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "category": {
                        "type": "string",
                        "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]
                    }
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_create",
            "description": "仅供目标团队明确指定的项目经理创建共享资产；team_ref 必须来自当前 Run 的 agent_workspace_snapshot。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                    "title": {"type": "string", "minLength": 1, "maxLength": 240},
                    "markdown": {"type": "string", "maxLength": 128000}
                },
                "required": ["team_ref", "category", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_update",
            "description": "仅供目标团队明确指定的项目经理使用当前 Run 的 asset_ref 更新共享资产；revision 由引用携带并由客户端并发校验。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160},
                    "category": {"type": "string", "enum": ["overview", "current_progress", "tech_stack", "architecture", "conventions", "decision", "reference"]},
                    "title": {"type": "string", "minLength": 1, "maxLength": 240},
                    "markdown": {"type": "string", "maxLength": 128000}
                },
                "required": ["asset_ref", "category", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_archive",
            "description": "仅供目标团队明确指定的项目经理使用当前 Run 的 asset_ref 归档共享资产；revision 由引用携带并由客户端并发校验。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160}
                },
                "required": ["asset_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_get",
            "description": "读取 team_asset_list 返回的当前 Run 共享资产引用。不要猜测或复用其他 Run 的 asset_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 160}
                },
                "required": ["asset_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_review_list",
            "description": "仅供项目经理列出执行者提交的待审核共享资产建议。检查当前项目经理负责的团队时必须省略 team_ref；只有在当前 Run 刚调用 agent_workspace_snapshot 且需要限定其中某一团队时，才传它返回的 team_ref，绝不能复用历史对话或 Memory 中的引用。每条建议必须应用到共享资产，或给出事实理由明确拒绝。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600}
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "team_asset_review_resolve",
            "description": "仅供项目经理处理一条共享资产建议。applied 时必须先用 team_asset_create/update 写入，再传成功返回的当前 asset_ref；rejected 时必须传具体 reason。处理后不可撤回。",
            "parameters": {
                "type": "object",
                "properties": {
                    "review_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "outcome": {"type": "string", "enum": ["applied", "rejected"]},
                    "asset_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "reason": {"type": "string", "minLength": 1, "maxLength": 4000}
                },
                "required": ["review_ref", "outcome"],
                "additionalProperties": false
            }
        }),
    ]
}

pub(crate) fn project_dashboard_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "project_dashboard_get",
            "description": "读取项目总览、里程碑、项目经理登记的问题，以及团队 Todo 的实时状态。项目经理更新看板前必须先调用本工具。",
            "parameters": {
                "type": "object",
                "properties": {"team_ref": {"type": "string", "minLength": 1, "maxLength": 600}},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "project_dashboard_update",
            "description": "仅供目标团队明确指定的项目经理更新结构化项目总览。必须先读取当前总览；首次创建省略 expected_revision，后续更新必须提交当前 revision。Todo 只能使用同一 Run 返回的 todo_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "expected_revision": {"type": "integer", "minimum": 1},
                    "phase": {"type": "string", "minLength": 1, "maxLength": 240},
                    "health": {"type": "string", "enum": ["on_track", "at_risk", "blocked", "completed"]},
                    "summary": {"type": "string", "minLength": 1, "maxLength": 16000},
                    "next_steps": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 2000}, "maxItems": 32},
                    "milestones": {
                        "type": "array", "maxItems": 64,
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string", "minLength": 1, "maxLength": 512},
                                "title": {"type": "string", "minLength": 1, "maxLength": 240},
                                "detail": {"type": "string", "maxLength": 8000},
                                "status": {"type": "string", "enum": ["pending", "in_progress", "blocked", "completed"]},
                                "progress_percent": {"type": "integer", "minimum": 0, "maximum": 100},
                                "acceptance_criteria": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 2000}, "maxItems": 32},
                                "todo_refs": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 600}, "maxItems": 128, "uniqueItems": true},
                                "target_at_unix_ms": {"type": "integer", "minimum": 0}
                            },
                            "required": ["id", "title", "status", "progress_percent"],
                            "additionalProperties": false
                        }
                    },
                    "issues": {
                        "type": "array", "maxItems": 64,
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string", "minLength": 1, "maxLength": 512},
                                "title": {"type": "string", "minLength": 1, "maxLength": 240},
                                "detail": {"type": "string", "maxLength": 8000},
                                "requested_action": {"type": "string", "minLength": 1, "maxLength": 4000},
                                "severity": {"type": "string", "enum": ["info", "warning", "critical"]},
                                "owner": {"type": "string", "enum": ["human", "agent", "external"]},
                                "todo_ref": {"type": "string", "minLength": 1, "maxLength": 600}
                            },
                            "required": ["id", "title", "requested_action", "severity", "owner"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["team_ref", "phase", "health", "summary"],
                "additionalProperties": false
            }
        }),
    ]
}
