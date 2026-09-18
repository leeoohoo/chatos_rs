use super::*;

pub(super) fn username_definition(now: &str) -> ConfigDefinitionRecord {
    definition(
        USER_SERVICE_SUPER_ADMIN_USERNAME_CONFIG_KEY,
        "超级管理员用户名",
        "User Service 用于匹配既有超级管理员或执行显式本地空库引导的用户名",
        "User Service / Bootstrap",
        "service",
        Some("user-service"),
        "string",
        json!("admin"),
        None,
        None,
        &[],
        "restart_required",
        &["USER_SERVICE_SUPER_ADMIN_USERNAME"],
        37501,
        now,
    )
}

pub(super) fn password_definition(now: &str) -> ConfigDefinitionRecord {
    secret_definition(
        USER_SERVICE_SUPER_ADMIN_PASSWORD_CONFIG_KEY,
        "超级管理员密码",
        "仅在显式允许空库本地引导时创建超级管理员使用的密码",
        "User Service / Bootstrap",
        "service",
        Some("user-service"),
        json!("admin123456"),
        "restart_required",
        &["USER_SERVICE_SUPER_ADMIN_PASSWORD"],
        37502,
        now,
    )
}

pub(super) fn display_name_definition(now: &str) -> ConfigDefinitionRecord {
    definition(
        USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME_CONFIG_KEY,
        "超级管理员显示名",
        "仅在显式允许空库本地引导时创建的超级管理员显示名称",
        "User Service / Bootstrap",
        "service",
        Some("user-service"),
        "string",
        json!("System Admin"),
        None,
        None,
        &[],
        "restart_required",
        &["USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME"],
        37503,
        now,
    )
}

pub(super) fn empty_database_gate_definition(now: &str) -> ConfigDefinitionRecord {
    definition(
        USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION_CONFIG_KEY,
        "允许空用户库创建管理员",
        "仅供非生产环境显式引导空用户库；生产环境即使开启也会拒绝启动",
        "User Service / Bootstrap",
        "service",
        Some("user-service"),
        "boolean",
        json!(false),
        None,
        None,
        &[],
        "restart_required",
        &["USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION"],
        375035,
        now,
    )
}
