use super::*;
use crate::agent_professions::PROFESSION_KEYS;

pub(crate) fn team_chat_read_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "agent_workspace_snapshot",
            "description": "读取当前账户的活动 Agent 与项目团队，并返回只对当前 Run 有效的 agent_ref、team_ref 和 project_ref。主动私聊另一个 Agent 时把 agent_ref 作为 target_agent_ref，主动向另一个项目团队发送消息时把 team_ref 交给 chat_send_message；真实 ID 不会暴露。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_list_members",
            "description": "列出当前项目团队或 Agent 私聊的活动成员，并返回仅对当前 Run 有效的 agent_ref。真实 Agent ID 不会暴露。",
            "parameters": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_read_all_unread",
            "description": "读取当前 Agent 在全部项目团队和 Agent 私聊中的未读消息，并在同一事务推进已返回会话的游标。回复时只把本工具返回的 message_ref 传给 chat_send_message。",
            "parameters": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 200}
                },
                "required": [],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_read_attachment",
            "description": "读取 chat_read_all_unread 返回的附件。message_ref 与 attachment_ref 必须来自同一条当前 Run 已签发的消息；文本按字符分页返回。当前触发消息中的二进制附件已作为多模态输入提供，历史二进制附件只返回元数据和重新附带提示。",
            "parameters": {
                "type": "object",
                "properties": {
                    "message_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "attachment_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "offset": {"type": "integer", "minimum": 0, "maximum": 2097152, "default": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 12000, "default": 12000}
                },
                "required": ["message_ref", "attachment_ref"],
                "additionalProperties": false
            }
        }),
    ]
}

pub(crate) fn team_chat_write_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "chat_document_create",
            "description": "创建当前 Run 内的 UTF-8 Markdown 文档草稿。客户端清洗文件名、计算大小和 SHA-256，只返回临时 document_ref；创建后必须在同一 Run 的下一条发送消息中通过 document_refs 附加。",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "minLength": 1, "maxLength": 240},
                    "title": {"type": "string", "minLength": 1, "maxLength": 512},
                    "markdown": {"type": "string", "minLength": 1, "maxLength": 2097152}
                },
                "required": ["name", "title", "markdown"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_propose_member",
            "description": "使用已授予的人员管理权限，向 Human 提交新 Agent 草案。模型配置由客户端继承当前 Agent；该工具只创建待审批提案，绝不会直接创建 Agent。",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "minLength": 1, "maxLength": 120},
                    "role": {"type": "string", "minLength": 1, "maxLength": 160},
                    "responsibility": {"type": "string", "maxLength": 8000},
                    "role_prompt": {"type": "string", "minLength": 1, "maxLength": 32000},
                    "thinking_level": {"type": "string", "enum": ["auto", "none", "minimal", "low", "medium", "high", "xhigh", "max"]},
                    "profession_key": {"type": "string", "enum": PROFESSION_KEYS},
                    "rationale": {"type": "string", "maxLength": 4000}
                },
                "required": ["name", "role", "role_prompt", "profession_key"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_propose_existing_member",
            "description": "使用人员管理权限，提交把账户中已有 Agent 加入指定项目团队的待审批提案。team_ref 与 target_agent_ref 必须来自当前 Run 的 agent_workspace_snapshot。",
            "parameters": {
                "type": "object",
                "properties": {
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "target_agent_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "role": {"type": "string", "minLength": 1, "maxLength": 160},
                    "responsibility": {"type": "string", "maxLength": 8000}
                },
                "required": ["team_ref", "target_agent_ref", "role"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_propose_member_removal",
            "description": "使用人员管理权限，提交把一个 Agent 移出当前项目团队的待审批提案。不会删除可复用 Agent profile，也不会绕过 Human 确认。",
            "parameters": {
                "type": "object",
                "properties": {
                    "target_agent_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "reason": {"type": "string", "minLength": 1, "maxLength": 4000},
                    "handoff_plan": {"type": "string", "maxLength": 8000}
                },
                "required": ["target_agent_ref", "reason"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_send_message",
            "description": "向当前会话发送消息，使用 chat_read_all_unread 返回的 message_ref 回复指定消息，使用 agent_workspace_snapshot 返回的 target_agent_ref 主动私聊并唤醒另一个 Agent，或用 team_ref 主动发送到另一个项目团队。reply_to_message_ref、target_agent_ref 和 team_ref 最多提供一个；需要 @ 其他成员时，只能使用当前 Run 返回且属于目标会话的 agent_ref。",
            "parameters": {
                "type": "object",
                "properties": {
                    "content": {"type": "string", "minLength": 1, "maxLength": 2000},
                    "reply_to_message_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "target_agent_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "team_ref": {"type": "string", "minLength": 1, "maxLength": 600},
                    "document_refs": {
                        "type": "array",
                        "items": {"type": "string", "minLength": 1, "maxLength": 600},
                        "maxItems": 5,
                        "uniqueItems": true
                    },
                    "mention_agent_refs": {
                        "type": "array",
                        "items": {"type": "string", "minLength": 1, "maxLength": 600},
                        "maxItems": 64,
                        "uniqueItems": true
                    }
                },
                "required": ["content"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "chat_heartbeat_complete",
            "description": "仅用于主动巡检：检查全部会话未读和任务后没有需要汇报或执行的事项时，安静完成本次巡检。",
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": "agent_cycle_complete",
            "description": "结束本次 Agent 通讯周期。调用前必须完成必要回复和任务调整；不会向聊天记录写入内部状态消息。",
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
    ]
}
