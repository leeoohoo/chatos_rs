fn runtime_clock_context() -> Value {
    let now = chrono::Local::now();
    message_item(
        "system",
        Value::String(
            json!({
                "type": "runtime_clock_context",
                "current_date": now.format("%Y-%m-%d").to_string(),
                "current_time": now.to_rfc3339(),
                "timezone": "host-local",
                "utc_offset": now.format("%:z").to_string(),
                "current_time_utc": now.with_timezone(&chrono::Utc).to_rfc3339()
            })
            .to_string(),
        ),
    )
}

fn bound_remote_connection_context(input: &Value, main_chat: bool) -> Value {
    // A current conversation setting (including explicit null) supersedes legacy
    // top-level data. Tasks use the connection frozen when their graph was made.
    let connection = if main_chat {
        input
            .pointer("/runtime_settings/remote_connection_id")
            .or_else(|| input.get("remote_connection_id"))
    } else {
        input.get("remote_connection_id")
    }
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|id| !id.is_empty());
    message_item("system", Value::String(json!({
        "type": "bound_remote_connection_context",
        "context_authority": "current_run",
        "remote_connection_id": connection,
        "target_selected": connection.is_some(),
        "guidance": if connection.is_some() {
            "The user has already selected a remote server in ChatOS. This program-owned connection id is the authoritative remote target for this Run; it is not a missing requirement. Do not ask the user to re-enter its hostname, IP, connection name, credentials, or connection id in a survey. The selection makes RemoteConnectionController available but does not replace, relocate, or invalidate the current ChatOS project and its program-bound local root. Use project tools for requests about the current project's files, Git history, build, or tests. Select RemoteConnectionController only when the user's request actually requires operating on or inspecting the selected remote host; the Host inherits and binds the connection automatically. Use authorized read-only discovery to identify remote services and configuration paths only for such remote work. A server selection alone does not require a Requirement Survey. Never infer authorization for unrelated changes or print secret values."
        } else {
            "No remote server is bound to this Run. Historical conversation text, plans and Memory do not establish a current connection. If remote access is needed, ask the user to select the target server in ChatOS; never guess a target or request credentials in a survey."
        }
    }).to_string()))
}

fn bound_project_context(input: &Value) -> Value {
    let project_id = input
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let project_name = input
        .get("project_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    message_item("system", Value::String(json!({
        "type": "bound_project_context",
        "context_authority": "current_run",
        "project_id": project_id,
        "project_name": project_name,
        "project_bound": project_id.is_some(),
        "guidance": if project_id.is_some() {
            "This Run is bound to the current ChatOS project. Tasks using project tools resolve that exact project's local root programmatically, and path `.` is that root. Do not claim that the project path or repository is unknown, do not scan unrelated directories to guess the project, and do not ask the user to identify the project again. A separately selected remote server does not change this project binding."
        } else {
            "No ChatOS project is bound to this Run. Do not infer a current project from historical conversation text or Memory."
        }
    }).to_string()))
}

const TASK_EXECUTION_WORKSPACE_INSTRUCTIONS: &str = "[Local Workspace Boundary]\nWhen project tools are available, path `.` is the authoritative root currently bound to this conversation. Treat the entries returned by `list_dir` for `.` as the root contents. Never replace that root with a child directory merely because its name appears to match the task or project; descend into a child only when the user's request requires inspecting that child, and keep root-relative paths and the distinction between the bound root and nested directories explicit in the final answer.";

const AGENT_PROJECT_PERMISSION: &str = "local.project.list";
const LEGACY_AGENT_PROJECT_PERMISSION: &str = "builtin.project-steward";
const AGENT_TASK_TOOLS: [&str; 8] = [
    "list_tasks",
    "get_task",
    "create_task",
    "create_tasks_with_prerequisites",
    "cancel_task",
    "wait_for_task_completion",
    "get_task_dependency_graph",
    "resolve_task_followup",
];

fn agent_direct_instructions(input: &Value) -> Result<Option<String>, String> {
    let Some(context) = input.get("agent_execution_context") else {
        return Ok(None);
    };
    let required = |field: &str| {
        context
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Agent execution context is missing {field}"))
    };
    let agent_name = required("agent_name")?;
    let agent_id = required("agent_id")?;
    let room_id = required("room_id")?;
    let role_prompt = required("role_prompt")?;
    let profession_key = required("profession_key")?;
    let profession_label = required("profession_label")?;
    let skill_ref = required("profession_skill_ref")?;
    let skill_sha256 = required("profession_skill_sha256")?;
    let skill_markdown = required("profession_skill_markdown")?;
    let permissions = context
        .get("default_skill_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "Agent execution context is missing default_skill_ids".to_string())?;
    let plugins = context
        .get("default_plugin_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "Agent execution context is missing default_plugin_ids".to_string())?;
    if context.get("schema_version").and_then(Value::as_u64) == Some(2) {
        let project_id = required("project_id")?;
        let team_name = required("team_name")?;
        let team_goal = required("team_goal")?;
        let team_role = required("team_role")?;
        let team_responsibility = required("team_responsibility")?;
        let is_project_manager = context
            .get("is_project_manager")
            .and_then(Value::as_bool)
            .ok_or_else(|| "Agent execution context is missing is_project_manager".to_string())?;
        return Ok(Some(format!(
            "[Bound ChatOS Team Agent Identity]\nAgent name: {agent_name}\nAgent id: {agent_id}\nTeam room id: {room_id}\nProject id: {project_id}\nTeam: {team_name}\nTeam goal: {team_goal}\nCurrent team role: {team_role}\nCurrent team responsibility: {team_responsibility}\nProject manager: {is_project_manager}\nProfession: {profession_label} ({profession_key})\nProfession Skill snapshot: {skill_ref}\nProfession Skill SHA-256: {skill_sha256}\nBound permission ids: {}\nBound plugin ids: {}\n\nIdentity and authority rules:\n- You are the bound member of the named project team, not the default Main Chat assistant and not a private-chat Agent. Keep this identity for the entire Run.\n- Work from the Human or Agent message delivered to this team lane and produce a concrete team response grounded in the actual result.\n- Your team role and responsibility are scoped to this team. Do not claim project-manager authority when Project manager is false.\n- The native client selected your model and thinking level from the persisted Agent profile. Never claim a different model or identity.\n- Permission ids and plugin ids are frozen data, not authority by themselves. Only tools actually present in this Run may be used.\n- Do not invent teammates, project state, completed work, messages, Todo items, or side effects.\n\n[Agent role prompt]\n{role_prompt}\n\n[Bound profession Skill]\n{skill_markdown}",
            Value::Array(permissions.clone()),
            Value::Array(plugins.clone()),
        )));
    }
    Ok(Some(format!(
        "[Bound ChatOS Agent Identity]\nAgent name: {agent_name}\nAgent id: {agent_id}\nDirect room id: {room_id}\nProfession: {profession_label} ({profession_key})\nProfession Skill snapshot: {skill_ref}\nProfession Skill SHA-256: {skill_sha256}\nBound permission ids: {}\nBound plugin ids: {}\n\nIdentity and authority rules:\n- You are the bound Agent above, not the default Main Chat assistant. Keep this identity for the entire Run.\n- The native client selected your model and thinking level from the persisted Agent profile. Never claim a different model or identity.\n- Permission ids and plugin ids are frozen data, not authority by themselves. Only tools actually present in this Run may be used.\n- Do not claim access to a project, staff management, a Plugin, or a side effect unless both the bound permission and an actual tool allow it.\n- Answer the Human in this private conversation. Do not invent another Agent or route the message through a Contact.\n\n[Agent role prompt]\n{role_prompt}\n\n[Bound profession Skill]\n{skill_markdown}",
        Value::Array(permissions.clone()),
        Value::Array(plugins.clone()),
    )))
}

fn scope_agent_direct_tools(mut tools: Vec<Value>, input: &Value) -> Vec<Value> {
    let Some(context) = input.get("agent_execution_context") else {
        tools.retain(|tool| {
            tool_name(tool).is_none_or(|name| {
                !name.starts_with("team_asset_") && !TEAM_EXECUTOR_TOOLS.contains(&name)
            })
        });
        return tools;
    };
    let is_team_executor = input.get("team_executor_context").is_some();
    let is_team_agent = context.get("schema_version").and_then(Value::as_u64) == Some(2);
    let is_project_manager = context
        .get("is_project_manager")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let permissions = context
        .get("default_skill_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<std::collections::HashSet<_>>();
    let can_access_projects = permissions.contains(AGENT_PROJECT_PERMISSION)
        || permissions.contains(LEGACY_AGENT_PROJECT_PERMISSION);
    if !can_access_projects {
        tools.retain(|tool| tool_name(tool).is_none_or(|name| !AGENT_TASK_TOOLS.contains(&name)));
    }
    tools.retain(|tool| {
        tool_name(tool).is_none_or(|name| {
            if TEAM_EXECUTOR_TOOLS.contains(&name) {
                is_team_agent && is_team_executor
            } else if matches!(name, "team_asset_list" | "team_asset_get") {
                is_team_agent
            } else if matches!(
                name,
                "team_asset_create" | "team_asset_update" | "team_asset_archive"
            ) {
                is_team_agent && is_project_manager
            } else {
                true
            }
        })
    });
    tools
}

const TEAM_EXECUTOR_TOOLS: [&str; 5] = [
    "todo_get_context",
    "todo_progress_append",
    "todo_acceptance_update",
    "todo_complete",
    "todo_block",
];
