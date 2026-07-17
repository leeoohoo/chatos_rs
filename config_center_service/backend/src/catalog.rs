// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{
    AGENT_MAX_ITERATIONS_CONFIG_KEY, AGENT_MAX_ITERATIONS_ENV, DEFAULT_AGENT_MAX_ITERATIONS,
    LEGACY_CHATOS_MAX_ITERATIONS_ENV, LEGACY_TASK_RUNNER_MAX_ITERATIONS_ENV,
};
use chrono::Utc;
use memory_engine_sdk::{
    memory_policy_config_key, memory_policy_env_key, ManagedMemoryPolicy, MemoryPolicyKind,
};
use serde_json::{json, Value};

use crate::models::ConfigDefinitionRecord;

pub const USER_PREFERENCE_CONFIG_KEYS: &[&str] =
    &["shared.ui.locale", "shared.ai.internal_context_locale"];
pub const LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS: &[&str] = &[
    "chatos.ai.max_iterations",
    "task_runner.execution.max_iterations",
];

pub fn builtin_definitions() -> Vec<ConfigDefinitionRecord> {
    let now = Utc::now().to_rfc3339();
    let mut definitions = vec![
        definition(
            "shared.logging.level",
            "日志级别",
            "服务默认日志级别",
            "日志",
            "shared",
            None,
            "enum",
            json!("info"),
            None,
            None,
            &["trace", "debug", "info", "warn", "error"],
            "restart_required",
            &["LOG_LEVEL"],
            30,
            &now,
        ),
        definition(
            AGENT_MAX_ITERATIONS_CONFIG_KEY,
            "Agent 最大迭代次数",
            "所有系统 Agent 单次执行的模型工具循环迭代上限",
            "Agent / Runtime",
            "shared",
            None,
            "integer",
            json!(DEFAULT_AGENT_MAX_ITERATIONS),
            Some(1),
            Some(5000),
            &[],
            "next_request",
            &[
                AGENT_MAX_ITERATIONS_ENV,
                LEGACY_CHATOS_MAX_ITERATIONS_ENV,
                LEGACY_TASK_RUNNER_MAX_ITERATIONS_ENV,
            ],
            100,
            &now,
        ),
        definition(
            "chatos.ai.max_output_tokens",
            "最大输出 Tokens",
            "单次模型回复最大输出 Tokens，空值表示模型默认",
            "Chat OS / AI",
            "service",
            Some("chatos-backend"),
            "integer",
            Value::Null,
            Some(1),
            Some(1_000_000),
            &[],
            "next_request",
            &["CHAT_MAX_TOKENS"],
            110,
            &now,
        ),
        definition(
            "chatos.task.follow_up_max_rounds",
            "任务后置检查轮数",
            "任务准备结束时最多继续检查的轮数",
            "Chat OS / Task",
            "service",
            Some("chatos-backend"),
            "integer",
            json!(3),
            Some(0),
            Some(100),
            &[],
            "next_request",
            &["TASK_FOLLOW_UP_MAX_ROUNDS"],
            120,
            &now,
        ),
        definition(
            "chatos.conversation.history_limit",
            "历史消息数量",
            "请求模型时读取的历史消息数量",
            "Chat OS / Conversation",
            "service",
            Some("chatos-backend"),
            "integer",
            json!(20),
            Some(1),
            Some(1000),
            &[],
            "next_request",
            &["HISTORY_LIMIT"],
            130,
            &now,
        ),
        definition(
            "chatos.attachment.total_max_bytes",
            "附件总大小上限",
            "单次消息所有附件原始文件大小之和",
            "Chat OS / Attachment",
            "service",
            Some("chatos-backend"),
            "bytes",
            json!(20 * 1024 * 1024),
            Some(1),
            Some(1024 * 1024 * 1024),
            &[],
            "hot_reload",
            &["ATTACHMENT_TOTAL_MAX_BYTES"],
            140,
            &now,
        ),
        definition(
            "chatos.ui.terminal_enabled",
            "显示终端入口",
            "是否在 Chat OS 中显示终端菜单和视图",
            "Chat OS / UI",
            "service",
            Some("chatos-backend"),
            "boolean",
            json!(true),
            None,
            None,
            &[],
            "hot_reload",
            &["TERMINAL_UI_ENABLED"],
            150,
            &now,
        ),
        definition(
            "task_runner.execution.timeout_ms",
            "任务执行超时",
            "单次 Task Run 执行超时毫秒数",
            "Task Runner / Execution",
            "service",
            Some("task-runner"),
            "duration_ms",
            json!(7_200_000),
            Some(1000),
            Some(86_400_000),
            &[],
            "next_run",
            &["TASK_RUNNER_EXECUTION_TIMEOUT_MS"],
            210,
            &now,
        ),
        definition(
            "task_runner.ai.tool_result_max_chars",
            "单工具结果字符上限",
            "单个工具结果进入模型上下文的最大字符数",
            "Task Runner / AI",
            "service",
            Some("task-runner"),
            "integer",
            json!(8000),
            Some(1),
            Some(10_000_000),
            &[],
            "next_run",
            &["CHATOS_AI_TOOL_RESULT_MODEL_MAX_CHARS"],
            220,
            &now,
        ),
        definition(
            "task_runner.ai.tool_results_total_max_chars",
            "工具结果总字符上限",
            "一轮所有工具结果进入模型上下文的总字符预算",
            "Task Runner / AI",
            "service",
            Some("task-runner"),
            "integer",
            json!(48000),
            Some(1),
            Some(100_000_000),
            &[],
            "next_run",
            &["CHATOS_AI_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS"],
            230,
            &now,
        ),
        definition(
            "task_runner.sandbox.enabled",
            "启用任务沙箱",
            "允许 Task Runner 按策略申请沙箱",
            "Task Runner / Sandbox",
            "service",
            Some("task-runner"),
            "boolean",
            json!(true),
            None,
            None,
            &[],
            "next_run",
            &["TASK_RUNNER_SANDBOX_ENABLED"],
            240,
            &now,
        ),
        definition(
            "task_runner.sandbox.manager_base_url",
            "沙箱管理服务地址",
            "Task Runner 访问 Sandbox Manager 的基础地址",
            "Task Runner / Sandbox",
            "service",
            Some("task-runner"),
            "string",
            json!("http://127.0.0.1:8095"),
            None,
            None,
            &[],
            "next_run",
            &["TASK_RUNNER_SANDBOX_MANAGER_BASE_URL"],
            245,
            &now,
        ),
        definition(
            "task_runner.sandbox.lease_ttl_seconds",
            "沙箱租约 TTL",
            "Task Runner 沙箱租约默认秒数",
            "Task Runner / Sandbox",
            "service",
            Some("task-runner"),
            "integer",
            json!(7200),
            Some(60),
            Some(86400),
            &[],
            "next_run",
            &["TASK_RUNNER_SANDBOX_LEASE_TTL_SECONDS"],
            250,
            &now,
        ),
        definition(
            "task_runner.worker.concurrency",
            "Worker 并发数",
            "单个 Task Runner Worker 实例并发执行数",
            "Task Runner / Worker",
            "service",
            Some("task-runner"),
            "integer",
            json!(4),
            Some(1),
            Some(256),
            &[],
            "restart_required",
            &["TASK_RUNNER_WORKER_CONCURRENCY"],
            260,
            &now,
        ),
        definition(
            "task_runner.worker.claim_ttl_ms",
            "Worker Claim TTL",
            "Worker 对 Run 的占用租约毫秒数",
            "Task Runner / Worker",
            "service",
            Some("task-runner"),
            "duration_ms",
            json!(120000),
            Some(1000),
            Some(3_600_000),
            &[],
            "next_claim",
            &["TASK_RUNNER_WORKER_CLAIM_TTL_MS"],
            270,
            &now,
        ),
        definition(
            "task_runner.worker.poll_interval_ms",
            "Worker 轮询间隔",
            "Worker 查询待执行任务的间隔",
            "Task Runner / Worker",
            "service",
            Some("task-runner"),
            "duration_ms",
            json!(1000),
            Some(50),
            Some(60000),
            &[],
            "hot_reload",
            &["TASK_RUNNER_WORKER_POLL_MS"],
            280,
            &now,
        ),
    ];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_exposes_one_shared_agent_iteration_limit() {
        let definitions = builtin_definitions();
        let iteration_definitions = definitions
            .iter()
            .filter(|definition| definition.key.contains("max_iterations"))
            .collect::<Vec<_>>();

        assert_eq!(iteration_definitions.len(), 1);
        let definition = iteration_definitions[0];
        assert_eq!(definition.key, AGENT_MAX_ITERATIONS_CONFIG_KEY);
        assert_eq!(definition.scope, "shared");
        assert_eq!(definition.service_name, None);
        assert_eq!(
            definition.default_value,
            json!(DEFAULT_AGENT_MAX_ITERATIONS)
        );
    }

    #[test]
    fn catalog_exposes_shared_memory_policies_for_server_and_client() {
        let definitions = builtin_definitions();
        let memory_definitions = definitions
            .iter()
            .filter(|definition| definition.key.starts_with("memory_engine.policy."))
            .collect::<Vec<_>>();

        assert!(!memory_definitions.is_empty());
        assert!(memory_definitions
            .iter()
            .all(|definition| definition.scope == "shared"));
        assert!(memory_definitions
            .iter()
            .all(|definition| !definition.key.ends_with("model_profile_id")));
        assert!(memory_definitions.iter().any(|definition| {
            definition.key == "memory_engine.policy.rollup.keep_level0_count"
                && definition.default_value == json!(5)
        }));
        assert!(memory_definitions.iter().any(|definition| {
            definition.key == "memory_engine.policy.thread_repair.token_limit"
                && definition.default_value == json!(200000)
        }));
    }
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
