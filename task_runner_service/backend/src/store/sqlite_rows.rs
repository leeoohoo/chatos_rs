use sqlx::{sqlite::SqliteRow, Row};

use crate::models::{
    ExternalMcpConfigRecord, ModelConfigRecord, RemoteServerRecord, RunSummaryRecord,
    RuntimeSettingsRecord, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskSummaryRecord,
    UiPromptRecord, UserRecord,
};

use super::codec::{
    decode_json, decode_json_option, decode_json_optional_typed, int_to_bool,
    task_run_status_from_str, task_status_from_str, ui_prompt_status_from_str, user_role_from_str,
};

pub(super) fn task_from_row(row: &SqliteRow) -> Result<TaskRecord, String> {
    Ok(TaskRecord {
        id: row.get("id"),
        title: row.get("title"),
        description: row.get("description"),
        objective: row.get("objective"),
        input_payload: decode_json_option(row.get("input_payload_json"))?,
        status: task_status_from_str(row.get::<String, _>("status").as_str()),
        priority: row.get::<i64, _>("priority") as i32,
        tags: decode_json(row.get("tags_json"))?,
        default_model_config_id: row.get("default_model_config_id"),
        memory_thread_id: row.get("memory_thread_id"),
        tenant_id: row.get("tenant_id"),
        subject_id: row.get("subject_id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        result_summary: row.get("result_summary"),
        process_log: row.get("process_log"),
        last_run_id: row.get("last_run_id"),
        schedule: decode_json(row.get("schedule_json"))?,
        parent_task_id: row.get("parent_task_id"),
        source_run_id: row.get("source_run_id"),
        source_session_id: row.get("source_session_id"),
        source_turn_id: row.get("source_turn_id"),
        source_user_message_id: row.get("source_user_message_id"),
        prerequisite_task_ids: Vec::new(),
        task_tool_state: decode_json(row.get("task_tool_state_json"))?,
        mcp_config: decode_json(row.get("mcp_config_json"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.get("deleted_at"),
    })
}

pub(super) fn task_summary_from_row(row: &SqliteRow) -> Result<TaskSummaryRecord, String> {
    Ok(TaskSummaryRecord {
        id: row.get("id"),
        title: row.get("title"),
        status: task_status_from_str(row.get::<String, _>("status").as_str()),
        default_model_config_id: row.get("default_model_config_id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        last_run_id: row.get("last_run_id"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn user_from_row(row: &SqliteRow) -> Result<UserRecord, String> {
    Ok(UserRecord {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        password_hash: row.get("password_hash"),
        role: user_role_from_str(row.get::<String, _>("role").as_str()),
        enabled: int_to_bool(row.get::<i64, _>("enabled")),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        last_login_at: row.get("last_login_at"),
    })
}

pub(super) fn model_config_from_row(row: &SqliteRow) -> Result<ModelConfigRecord, String> {
    Ok(ModelConfigRecord {
        id: row.get("id"),
        owner_user_id: row.get("owner_user_id"),
        name: row.get("name"),
        provider: row.get("provider"),
        base_url: row.get("base_url"),
        api_key: row.get("api_key"),
        model: row.get("model"),
        usage_scenario: row.get("usage_scenario"),
        temperature: row.get("temperature"),
        max_output_tokens: row.get("max_output_tokens"),
        thinking_level: row.get("thinking_level"),
        supports_responses: int_to_bool(row.get::<i64, _>("supports_responses")),
        instructions: row.get("instructions"),
        request_cwd: row.get("request_cwd"),
        include_prompt_cache_retention: int_to_bool(
            row.get::<i64, _>("include_prompt_cache_retention"),
        ),
        request_body_limit_bytes: row
            .get::<Option<i64>, _>("request_body_limit_bytes")
            .map(|value| value as usize),
        enabled: int_to_bool(row.get::<i64, _>("enabled")),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn runtime_settings_from_row(row: &SqliteRow) -> Result<RuntimeSettingsRecord, String> {
    Ok(RuntimeSettingsRecord {
        id: row.get("id"),
        task_execution_max_iterations: row.get::<i64, _>("task_execution_max_iterations") as usize,
        execution_timeout_ms: row
            .get::<Option<i64>, _>("execution_timeout_ms")
            .map(|value| value as u64),
        tool_result_model_max_chars: row.get::<i64, _>("tool_result_model_max_chars") as usize,
        tool_results_model_total_max_chars: row.get::<i64, _>("tool_results_model_total_max_chars")
            as usize,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn remote_server_from_row(row: &SqliteRow) -> Result<RemoteServerRecord, String> {
    Ok(RemoteServerRecord {
        id: row.get("id"),
        name: row.get("name"),
        host: row.get("host"),
        port: row.get("port"),
        username: row.get("username"),
        auth_type: row.get("auth_type"),
        password: row.get("password"),
        private_key_path: row.get("private_key_path"),
        certificate_path: row.get("certificate_path"),
        default_remote_path: row.get("default_remote_path"),
        host_key_policy: row.get("host_key_policy"),
        enabled: int_to_bool(row.get::<i64, _>("enabled")),
        last_tested_at: row.get("last_tested_at"),
        last_test_status: row.get("last_test_status"),
        last_test_message: row.get("last_test_message"),
        last_active_at: row.get("last_active_at"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        task_id: row.get("task_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn external_mcp_config_from_row(
    row: &SqliteRow,
) -> Result<ExternalMcpConfigRecord, String> {
    Ok(ExternalMcpConfigRecord {
        id: row.get("id"),
        name: row.get("name"),
        transport: row.get("transport"),
        command: row.get("command"),
        args: decode_json(row.get("args_json"))?,
        url: row.get("url"),
        headers: decode_json(row.get("headers_json"))?,
        env: decode_json(row.get("env_json"))?,
        cwd: row.get("cwd"),
        enabled: int_to_bool(row.get::<i64, _>("enabled")),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn task_run_from_row(row: &SqliteRow) -> Result<TaskRunRecord, String> {
    Ok(TaskRunRecord {
        id: row.get("id"),
        task_id: row.get("task_id"),
        model_config_id: row.get("model_config_id"),
        memory_thread_id: row.get("memory_thread_id"),
        status: task_run_status_from_str(row.get::<String, _>("status").as_str()),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        input_snapshot: decode_json(row.get("input_snapshot_json"))?,
        context_snapshot: decode_json_option(row.get("context_snapshot_json"))?,
        result_summary: row.get("result_summary"),
        error_message: row.get("error_message"),
        usage: decode_json_option(row.get("usage_json"))?,
        report: decode_json_option(row.get("report_json"))?,
        cancel_requested: int_to_bool(row.get::<i64, _>("cancel_requested")),
        summary_job_run_id: row.get("summary_job_run_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn run_summary_from_row(row: &SqliteRow) -> Result<RunSummaryRecord, String> {
    Ok(RunSummaryRecord {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: task_run_status_from_str(row.get::<String, _>("status").as_str()),
        model_config_id: row.get("model_config_id"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn task_run_event_from_row(row: &SqliteRow) -> Result<TaskRunEventRecord, String> {
    Ok(TaskRunEventRecord {
        id: row.get("id"),
        run_id: row.get("run_id"),
        event_type: row.get("event_type"),
        message: row.get("message"),
        payload: decode_json_option(row.get("payload_json"))?,
        created_at: row.get("created_at"),
    })
}

pub(super) fn ui_prompt_from_row(row: &SqliteRow) -> Result<UiPromptRecord, String> {
    Ok(UiPromptRecord {
        id: row.get("id"),
        task_id: row.get("task_id"),
        run_id: row.get("run_id"),
        conversation_id: row.get("conversation_id"),
        conversation_turn_id: row.get("conversation_turn_id"),
        tool_call_id: row.get("tool_call_id"),
        kind: row.get("kind"),
        title: row.get("title"),
        message: row.get("message"),
        allow_cancel: int_to_bool(row.get::<i64, _>("allow_cancel")),
        timeout_ms: row.get::<i64, _>("timeout_ms") as u64,
        payload: decode_json(row.get("payload_json"))?,
        response: decode_json_optional_typed(row.get("response_json"))?,
        status: ui_prompt_status_from_str(row.get::<String, _>("status").as_str()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        expires_at: row.get("expires_at"),
    })
}
