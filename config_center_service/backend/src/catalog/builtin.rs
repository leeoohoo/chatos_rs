use chatos_agent::{
    AGENT_MAX_ITERATIONS_CONFIG_KEY, AGENT_MAX_NO_PROGRESS_ROUNDS_CONFIG_KEY,
    AGENT_MAX_REQUEST_RETRIES_CONFIG_KEY, AGENT_REQUEST_TIMEOUT_SECONDS_CONFIG_KEY,
    AGENT_RUN_TIMEOUT_SECONDS_CONFIG_KEY, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_AGENT_MAX_NO_PROGRESS_ROUNDS, DEFAULT_AGENT_MAX_REQUEST_RETRIES,
    DEFAULT_AGENT_REQUEST_TIMEOUT_SECONDS, DEFAULT_AGENT_RUN_TIMEOUT_SECONDS,
    DEFAULT_LOCAL_TASK_EXECUTION_MAX_ITERATIONS, DEFAULT_LOCAL_TASK_EXECUTION_PROMPT_CACHE_ENABLED,
    DEFAULT_LOCAL_TASK_EXECUTION_PROMPT_CACHE_RETENTION_ENABLED,
    DEFAULT_LOCAL_TASK_EXECUTION_REVIEW_MISSING_READ_FAILURES,
    DEFAULT_LOCAL_TASK_EXECUTION_REVIEW_READ_ONLY_ITERATIONS,
    DEFAULT_LOCAL_TASK_EXECUTION_REVIEW_REPEAT_INTERVAL,
    LOCAL_TASK_EXECUTION_MAX_ITERATIONS_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_PROMPT_CACHE_ENABLED_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_PROMPT_CACHE_RETENTION_ENABLED_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_REVIEW_MISSING_READ_FAILURES_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_REVIEW_READ_ONLY_ITERATIONS_CONFIG_KEY,
    LOCAL_TASK_EXECUTION_REVIEW_REPEAT_INTERVAL_CONFIG_KEY,
};
use chrono::Utc;
use serde_json::{json, Value};

use super::constants::*;
use crate::models::ConfigDefinitionRecord;

#[path = "builtin/configuration_center.rs"]
mod configuration_center;
#[path = "builtin/local_connector.rs"]
mod local_connector;
#[path = "builtin/memory_engine.rs"]
mod memory_engine;
#[path = "builtin/plugin_management.rs"]
mod plugin_management;
#[path = "builtin/shared_agent.rs"]
mod shared_agent;
#[path = "builtin/user_service.rs"]
mod user_service;

pub fn builtin_definitions() -> Vec<ConfigDefinitionRecord> {
    let now = Utc::now().to_rfc3339();
    let mut definitions = Vec::new();
    definitions.extend(configuration_center::definitions(&now));
    definitions.extend(shared_agent::definitions(&now));
    definitions.extend(local_connector::definitions(&now));
    definitions.extend(plugin_management::definitions(&now));
    definitions.extend(memory_engine::definitions(&now));
    definitions.extend(user_service::definitions(&now));
    definitions.extend(postgres_definitions(&now));
    definitions
}

fn postgres_definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    let mut definitions = vec![
        definition(
            "platform.postgres.server_max_connections",
            "PostgreSQL Server Max Connections",
            "PostgreSQL 实例的 max_connections，用于发布前校验所有服务连接池预算",
            "Platform / PostgreSQL",
            "shared",
            None,
            "integer",
            json!(200),
            Some(20),
            Some(100_000),
            &[],
            "restart_required",
            &[],
            8_000,
            now,
        ),
        definition(
            "platform.postgres.reserved_connections",
            "PostgreSQL Reserved Connections",
            "为迁移、运维和 PostgreSQL 管理连接预留的连接数",
            "Platform / PostgreSQL",
            "shared",
            None,
            "integer",
            json!(40),
            Some(1),
            Some(10_000),
            &[],
            "restart_required",
            &[],
            8_001,
            now,
        ),
    ];
    for (namespace, service_name, env_prefix, display_name, replicas, order) in [
        (
            "configuration_center",
            "configuration-center",
            "CONFIG_CENTER",
            "Configuration Center",
            1,
            8_100,
        ),
        (
            "user_service",
            "user-service",
            "USER_SERVICE",
            "User Service",
            1,
            8_300,
        ),
        (
            "plugin_management",
            "plugin-management-service",
            "PLUGIN_MANAGEMENT",
            "Plugin Management",
            1,
            8_400,
        ),
        (
            "local_connector",
            "local-connector-service",
            "LOCAL_CONNECTOR",
            "Local Connector",
            1,
            8_500,
        ),
        (
            "memory_engine",
            "memory-engine",
            "MEMORY_ENGINE",
            "Memory Engine",
            2,
            8_800,
        ),
    ] {
        let category = format!("{display_name} / PostgreSQL");
        let fields = [
            (
                "pool.max_connections",
                "Pool Max Connections",
                "单个进程允许打开的最大 PostgreSQL 连接数",
                "integer",
                10,
                1,
                1_000,
                "POOL_MAX_CONNECTIONS",
            ),
            (
                "pool.min_connections",
                "Pool Min Connections",
                "单个进程保持的最小 PostgreSQL 空闲连接数",
                "integer",
                1,
                0,
                1_000,
                "POOL_MIN_CONNECTIONS",
            ),
            (
                "pool.acquire_timeout_ms",
                "Pool Acquire Timeout",
                "从连接池获取连接的最大等待毫秒数",
                "duration_ms",
                5_000,
                100,
                300_000,
                "POOL_ACQUIRE_TIMEOUT_MS",
            ),
            (
                "pool.idle_timeout_ms",
                "Pool Idle Timeout",
                "空闲连接回收前允许保留的毫秒数",
                "duration_ms",
                600_000,
                1_000,
                86_400_000,
                "POOL_IDLE_TIMEOUT_MS",
            ),
            (
                "pool.max_lifetime_ms",
                "Pool Max Lifetime",
                "单个连接允许存活的最大毫秒数",
                "duration_ms",
                1_800_000,
                1_000,
                86_400_000,
                "POOL_MAX_LIFETIME_MS",
            ),
            (
                "statement_timeout_ms",
                "Statement Timeout",
                "普通 SQL 语句允许执行的最大毫秒数",
                "duration_ms",
                30_000,
                100,
                7_200_000,
                "STATEMENT_TIMEOUT_MS",
            ),
            (
                "lock_timeout_ms",
                "Lock Timeout",
                "SQL 等待 PostgreSQL 锁的最大毫秒数",
                "duration_ms",
                5_000,
                100,
                300_000,
                "LOCK_TIMEOUT_MS",
            ),
        ];
        for (offset, (field, title, description, value_type, default, min, max, env_suffix)) in
            fields.into_iter().enumerate()
        {
            let key = format!("{namespace}.postgres.{field}");
            let env_alias = format!("{env_prefix}_POSTGRES_{env_suffix}");
            definitions.push(definition(
                key.as_str(),
                title,
                description,
                category.as_str(),
                "service",
                Some(service_name),
                value_type,
                json!(default),
                Some(min),
                Some(max),
                &[],
                "restart_required",
                &[env_alias.as_str()],
                order + i32::try_from(offset).unwrap_or_default(),
                now,
            ));
        }
        let replica_key = format!("{namespace}.postgres.process_replicas");
        definitions.push(definition(
            replica_key.as_str(),
            "PostgreSQL Pool Process Replicas",
            "会创建该服务 PostgreSQL 连接池的部署进程总副本数，用于连接预算校验",
            category.as_str(),
            "service",
            Some(service_name),
            "integer",
            json!(replicas),
            Some(1),
            Some(1_000),
            &[],
            "restart_required",
            &[],
            order + 7,
            now,
        ));
    }
    definitions
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
        nullable: false,
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
    let value_type = if default_value.is_string() {
        "string"
    } else {
        "json"
    };
    let mut record = definition(
        key,
        display_name,
        description,
        category,
        scope,
        service_name,
        value_type,
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
