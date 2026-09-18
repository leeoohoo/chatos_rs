use super::*;

pub(super) fn definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    vec![
        definition(
            USER_SERVICE_RETENTION_INTERVAL_SECONDS_CONFIG_KEY,
            "过期临时数据清理间隔（秒）",
            "User Service 分批物理删除过期验证码、票据、会话、nonce、撤销令牌和登录限流记录的间隔",
            "User Service / Retention",
            "service",
            Some("user-service"),
            "integer",
            json!(60),
            Some(10),
            Some(24 * 60 * 60),
            &[],
            "restart_required",
            &["USER_SERVICE_RETENTION_INTERVAL_SECONDS"],
            375081,
            now,
        ),
        definition(
            USER_SERVICE_RETENTION_BATCH_SIZE_CONFIG_KEY,
            "过期临时数据清理批量",
            "User Service 每轮、每张表最多物理删除的过期临时记录数",
            "User Service / Retention",
            "service",
            Some("user-service"),
            "integer",
            json!(500),
            Some(1),
            Some(10_000),
            &[],
            "restart_required",
            &["USER_SERVICE_RETENTION_BATCH_SIZE"],
            375082,
            now,
        ),
    ]
}
