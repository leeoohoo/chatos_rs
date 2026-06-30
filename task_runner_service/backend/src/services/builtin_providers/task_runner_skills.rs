use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{SkillRecord, UserRole};

use super::*;

const SEARCH_INSTALLED_SKILLS: &str = "search_installed_skills";
const GET_SKILL_DETAIL: &str = "get_skill_detail";

#[derive(Debug, Deserialize)]
struct SearchInstalledSkillsArgs {
    keyword: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SkillIdArgs {
    skill_id: String,
}

#[derive(Clone)]
pub(in crate::services) struct TaskRunnerSkillLookupProvider {
    server_name: String,
    skill_service: SkillService,
    current_user: CurrentUser,
}

impl TaskRunnerSkillLookupProvider {
    pub(in crate::services) fn new(
        server_name: impl Into<String>,
        skill_service: SkillService,
        owner_user_id: String,
    ) -> Self {
        Self {
            server_name: server_name.into(),
            skill_service,
            current_user: task_owner_agent_user(owner_user_id),
        }
    }
}

#[async_trait]
impl BuiltinToolProvider for TaskRunnerSkillLookupProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        vec![
            tool_definition(
                SEARCH_INSTALLED_SKILLS,
                "Search installed Task Runner skills visible to the current task owner, including bundled global skills and that owner's installed skills. Use returned id values as skill_ids when creating project tasks.",
                search_installed_skills_schema(),
            ),
            tool_definition(
                GET_SKILL_DETAIL,
                "Get full details for one installed Task Runner skill visible to the current task owner, including instructions and package file metadata.",
                get_skill_detail_schema(),
            ),
        ]
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match name {
            SEARCH_INSTALLED_SKILLS => {
                let args: SearchInstalledSkillsArgs = decode_args(args)?;
                let skills = self
                    .skill_service
                    .search_installed_skills_for_user(args.keyword, args.limit, &self.current_user)
                    .await?;
                Ok(text_result(skills_for_mcp(skills)))
            }
            GET_SKILL_DETAIL => {
                let args: SkillIdArgs = decode_args(args)?;
                let skill = self
                    .skill_service
                    .get_skill_for_user(args.skill_id.as_str(), &self.current_user)
                    .await?
                    .ok_or_else(|| format!("Skill 不存在或无权访问: {}", args.skill_id))?;
                Ok(text_result(skill_detail_for_mcp(skill)))
            }
            other => Err(format!(
                "unsupported Task Runner skill lookup tool: {other}"
            )),
        }
    }
}

fn task_owner_agent_user(owner_user_id: String) -> CurrentUser {
    CurrentUser {
        id: format!("task-runner-plan-agent-{owner_user_id}"),
        username: format!("task-runner-plan-agent-{owner_user_id}"),
        display_name: "Task Runner Plan Agent".to_string(),
        role: UserRole::Agent,
        owner_user_id: Some(owner_user_id.clone()),
        owner_username: Some(owner_user_id.clone()),
        owner_display_name: Some(owner_user_id),
    }
}

fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

fn search_installed_skills_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "keyword": {
                "type": "string",
                "description": "按任务领域、文件类型、工具名或能力关键词搜索，例如 docx、pdf、spreadsheet、browser、image、review。为空时返回当前 owner 可用的常用已安装 skills。"
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "description": "最多返回多少条，默认 20。"
            }
        },
        "additionalProperties": false
    })
}

fn get_skill_detail_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "skill_id": {
                "type": "string",
                "minLength": 1,
                "description": "search_installed_skills 或项目任务记录里返回的真实 Skill ID。"
            }
        },
        "required": ["skill_id"],
        "additionalProperties": false
    })
}

fn decode_args<T>(args: Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(args).map_err(|err| err.to_string())
}

fn text_result(payload: Value) -> Value {
    let text = if payload.is_string() {
        payload.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
    };
    let mut out = json!({
        "content": [
            { "type": "text", "text": text }
        ]
    });
    if !payload.is_string() && !payload.is_null() {
        out["_structured_result"] = payload;
    }
    out
}

fn skills_for_mcp(skills: Vec<SkillRecord>) -> Value {
    Value::Array(skills.into_iter().map(skill_summary_for_mcp).collect())
}

fn skill_summary_for_mcp(skill: SkillRecord) -> Value {
    json!({
        "id": skill.id,
        "name": skill.name,
        "display_name": skill.display_name,
        "description": skill.description,
        "locale": skill.locale,
        "tags": skill.tags,
        "source": skill.source,
        "scope": skill.scope,
        "enabled": skill.enabled,
        "auto_inject": skill.auto_inject,
        "package_file_count": skill.package_file_count,
        "package_total_bytes": skill.package_total_bytes,
        "source_repo": skill.source_repo,
        "source_ref": skill.source_ref,
        "source_path": skill.source_path,
        "content_preview": preview_skill_content(skill.content.as_str()),
    })
}

fn skill_detail_for_mcp(skill: SkillRecord) -> Value {
    json!({
        "id": skill.id,
        "name": skill.name,
        "display_name": skill.display_name,
        "description": skill.description,
        "content": skill.content,
        "locale": skill.locale,
        "tags": skill.tags,
        "source": skill.source,
        "scope": skill.scope,
        "enabled": skill.enabled,
        "auto_inject": skill.auto_inject,
        "install_status": skill.install_status,
        "package_file_count": skill.package_file_count,
        "package_total_bytes": skill.package_total_bytes,
        "package_manifest": skill.package_manifest,
        "source_repo": skill.source_repo,
        "source_ref": skill.source_ref,
        "source_path": skill.source_path,
        "source_url": skill.source_url,
    })
}

fn preview_skill_content(content: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 1200;
    let content = content.trim();
    if content.chars().count() <= MAX_PREVIEW_CHARS {
        return content.to_string();
    }
    format!(
        "{}\n...",
        content.chars().take(MAX_PREVIEW_CHARS).collect::<String>()
    )
}
