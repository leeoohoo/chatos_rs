use chatos_agent::{
    AGENT_MAX_ITERATIONS_CONFIG_KEY, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_TASK_RUNNER_PROMPT_CACHE_ENABLED, DEFAULT_TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED,
    DEFAULT_TASK_RUNNER_REVIEW_MISSING_READ_FAILURES,
    DEFAULT_TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS, DEFAULT_TASK_RUNNER_REVIEW_REPEAT_INTERVAL,
    TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY, TASK_RUNNER_PROMPT_CACHE_ENABLED_CONFIG_KEY,
    TASK_RUNNER_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    TASK_RUNNER_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
    TASK_RUNNER_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
    TASK_RUNNER_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
};
use chatos_service_runtime::DEFAULT_SANDBOX_MANAGER_AGENT_TOKEN_SECRET;
use chrono::Utc;
use memory_engine_sdk::{
    memory_policy_config_key, memory_policy_env_key, ManagedMemoryPolicy, MemoryPolicyKind,
};
use serde_json::{json, Value};

use super::constants::*;
use crate::models::ConfigDefinitionRecord;

#[path = "builtin/local_connector.rs"]
mod local_connector;
#[path = "builtin/mcp_management.rs"]
mod mcp_management;
#[path = "builtin/memory_engine.rs"]
mod memory_engine;
#[path = "builtin/plugin_management.rs"]
mod plugin_management;
#[path = "builtin/project_service.rs"]
mod project_service;
#[path = "builtin/sandbox_manager.rs"]
mod sandbox_manager;
#[path = "builtin/shared_chatos.rs"]
mod shared_chatos;
#[path = "builtin/task_runner.rs"]
mod task_runner;
#[path = "builtin/user_service.rs"]
mod user_service;

pub fn builtin_definitions() -> Vec<ConfigDefinitionRecord> {
    let now = Utc::now().to_rfc3339();
    let mut definitions = Vec::new();
    definitions.extend(shared_chatos::definitions(&now));
    definitions.extend(task_runner::definitions(&now));
    definitions.extend(sandbox_manager::definitions(&now));
    definitions.extend(local_connector::definitions(&now));
    definitions.extend(mcp_management::definitions(&now));
    definitions.extend(plugin_management::definitions(&now));
    definitions.extend(project_service::definitions(&now));
    definitions.extend(memory_engine::definitions(&now));
    definitions.extend(user_service::definitions(&now));
    definitions.extend(memory_policy_definitions(&now));
    definitions
}

fn memory_policy_definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    let mut definitions = Vec::new();
    for (kind, title, order) in [
        (MemoryPolicyKind::Summary, "消息总结", 400),
        (MemoryPolicyKind::Rollup, "总结聚合", 500),
        (MemoryPolicyKind::SubjectMemory, "主题记忆与记忆归并", 600),
        (MemoryPolicyKind::ThreadRepair, "修复总结", 700),
    ] {
        let defaults = kind.defaults();
        let category = format!("Memory Engine / {title}");
        definitions.push(memory_policy_definition(
            kind,
            "enabled",
            "启用",
            format!("是否启用{title}任务").as_str(),
            category.as_str(),
            "boolean",
            json!(defaults.enabled),
            None,
            None,
            false,
            order,
            now,
        ));
        push_optional_integer_definition(
            &mut definitions,
            kind,
            &defaults,
            "token_limit",
            "输入 Token 阈值",
            "单次处理或分块使用的输入 Token 上限",
            category.as_str(),
            128,
            2_000_000,
            order + 1,
            now,
        );
        push_optional_integer_definition(
            &mut definitions,
            kind,
            &defaults,
            "target_summary_tokens",
            "目标输出 Tokens",
            "模型生成总结或记忆时的目标输出 Token 数",
            category.as_str(),
            128,
            1_000_000,
            order + 2,
            now,
        );
        push_optional_integer_definition(
            &mut definitions,
            kind,
            &defaults,
            "interval_seconds",
            "调度间隔（秒）",
            "后台任务检查或刷新间隔",
            category.as_str(),
            3,
            86_400,
            order + 3,
            now,
        );
        push_optional_integer_definition(
            &mut definitions,
            kind,
            &defaults,
            "max_threads_per_tick",
            "每轮最大处理数",
            "单轮调度最多处理的线程或主题数量",
            category.as_str(),
            1,
            10_000,
            order + 4,
            now,
        );
        push_optional_integer_definition(
            &mut definitions,
            kind,
            &defaults,
            "count_limit",
            "聚合条数阈值",
            "达到该数量后允许执行聚合；0 表示仅按 Token 阈值判断",
            category.as_str(),
            0,
            1_000_000,
            order + 5,
            now,
        );
        push_optional_integer_definition(
            &mut definitions,
            kind,
            &defaults,
            "keep_level0_count",
            "保留 L0 数量",
            "执行聚合后保留的底层总结或记忆数量",
            category.as_str(),
            0,
            1_000_000,
            order + 6,
            now,
        );
        push_optional_integer_definition(
            &mut definitions,
            kind,
            &defaults,
            "max_level",
            "最大聚合层级",
            "总结或记忆允许向上聚合的最大层级",
            category.as_str(),
            1,
            128,
            order + 7,
            now,
        );
    }
    definitions
}

#[allow(clippy::too_many_arguments)]
fn push_optional_integer_definition(
    definitions: &mut Vec<ConfigDefinitionRecord>,
    kind: MemoryPolicyKind,
    defaults: &ManagedMemoryPolicy,
    field: &str,
    display_name: &str,
    description: &str,
    category: &str,
    min: i64,
    max: i64,
    ui_order: i32,
    now: &str,
) {
    let default_value = match field {
        "token_limit" => defaults.token_limit,
        "target_summary_tokens" => defaults.target_summary_tokens,
        "interval_seconds" => defaults.interval_seconds,
        "max_threads_per_tick" => defaults.max_threads_per_tick,
        "count_limit" => defaults.count_limit,
        "keep_level0_count" => defaults.keep_level0_count,
        "max_level" => defaults.max_level,
        _ => None,
    };
    let Some(default_value) = default_value else {
        return;
    };
    definitions.push(memory_policy_definition(
        kind,
        field,
        display_name,
        description,
        category,
        "integer",
        json!(default_value),
        Some(min),
        Some(max),
        false,
        ui_order,
        now,
    ));
}

#[allow(clippy::too_many_arguments)]
fn memory_policy_definition(
    kind: MemoryPolicyKind,
    field: &str,
    display_name: &str,
    description: &str,
    category: &str,
    value_type: &str,
    default_value: Value,
    min: Option<i64>,
    max: Option<i64>,
    nullable: bool,
    ui_order: i32,
    now: &str,
) -> ConfigDefinitionRecord {
    let key = memory_policy_config_key(kind, field);
    let env_alias = memory_policy_env_key(kind, field);
    let mut record = definition(
        key.as_str(),
        display_name,
        description,
        category,
        "shared",
        None,
        value_type,
        default_value,
        min,
        max,
        &[],
        "next_run",
        &[env_alias.as_str()],
        ui_order,
        now,
    );
    record.nullable = nullable;
    record
}

#[allow(clippy::too_many_arguments)]
fn definition(
    key: &str,
    display_name: &str,
    description: &str,
    category: &str,
    scope: &str,
    service_name: Option<&str>,
    value_type: &str,
    default_value: Value,
    min: Option<i64>,
    max: Option<i64>,
    enum_options: &[&str],
    reload_mode: &str,
    env_aliases: &[&str],
    ui_order: i32,
    now: &str,
) -> ConfigDefinitionRecord {
    ConfigDefinitionRecord {
        id: key.to_string(),
        key: key.to_string(),
        display_name: display_name.to_string(),
        description: description.to_string(),
        category: category.to_string(),
        scope: scope.to_string(),
        service_name: service_name.map(ToOwned::to_owned),
        value_type: value_type.to_string(),
        default_value,
        nullable: key == "chatos.ai.max_output_tokens",
        min,
        max,
        enum_options: enum_options
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        sensitivity: "public".to_string(),
        reload_mode: reload_mode.to_string(),
        criticality: "normal".to_string(),
        env_aliases: env_aliases
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        owner_team: "platform".to_string(),
        ui_order,
        deprecated: false,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn secret_definition(
    key: &str,
    display_name: &str,
    description: &str,
    category: &str,
    scope: &str,
    service_name: Option<&str>,
    default_value: Value,
    reload_mode: &str,
    env_aliases: &[&str],
    ui_order: i32,
    now: &str,
) -> ConfigDefinitionRecord {
    let mut record = definition(
        key,
        display_name,
        description,
        category,
        scope,
        service_name,
        "string",
        default_value,
        None,
        None,
        &[],
        reload_mode,
        env_aliases,
        ui_order,
        now,
    );
    record.sensitivity = "secret".to_string();
    record
}

#[allow(clippy::too_many_arguments)]
fn nullable_secret_definition(
    key: &str,
    display_name: &str,
    description: &str,
    category: &str,
    scope: &str,
    service_name: Option<&str>,
    default_value: Value,
    reload_mode: &str,
    env_aliases: &[&str],
    ui_order: i32,
    now: &str,
) -> ConfigDefinitionRecord {
    let mut record = nullable_definition(
        key,
        display_name,
        description,
        category,
        scope,
        service_name,
        "string",
        default_value,
        None,
        None,
        &[],
        reload_mode,
        env_aliases,
        ui_order,
        now,
    );
    record.sensitivity = "secret".to_string();
    record
}

#[allow(clippy::too_many_arguments)]
fn nullable_definition(
    key: &str,
    display_name: &str,
    description: &str,
    category: &str,
    scope: &str,
    service_name: Option<&str>,
    value_type: &str,
    default_value: Value,
    min: Option<i64>,
    max: Option<i64>,
    enum_options: &[&str],
    reload_mode: &str,
    env_aliases: &[&str],
    ui_order: i32,
    now: &str,
) -> ConfigDefinitionRecord {
    let mut record = definition(
        key,
        display_name,
        description,
        category,
        scope,
        service_name,
        value_type,
        default_value,
        min,
        max,
        enum_options,
        reload_mode,
        env_aliases,
        ui_order,
        now,
    );
    record.nullable = true;
    record
}
