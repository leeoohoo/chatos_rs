// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::auth::CurrentUser;
use crate::models::{now_rfc3339, TaskRecord};
use crate::store::AppStore;

use super::normalized_optional;

pub(super) fn resolve_task_tenant_id(
    requested_tenant_id: Option<String>,
    creator: Option<&CurrentUser>,
    default_tenant_id: &str,
) -> Result<String, String> {
    let requested_tenant_id = normalized_optional(requested_tenant_id);
    let Some(creator) = creator else {
        return Ok(requested_tenant_id.unwrap_or_else(|| default_tenant_id.to_string()));
    };

    if creator.is_admin() {
        return Ok(requested_tenant_id
            .or_else(|| creator.effective_owner_user_id().map(ToOwned::to_owned))
            .unwrap_or_else(|| default_tenant_id.to_string()));
    }

    creator
        .effective_owner_user_id()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "当前登录态缺少用户归属信息，无法创建任务租户".to_string())
}

pub(super) fn align_task_tenant_to_owner(task: &mut TaskRecord) -> bool {
    let Some(owner_user_id) = task
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    if task.tenant_id.trim() == owner_user_id {
        return false;
    }

    task.tenant_id = owner_user_id.to_string();
    true
}

pub(super) async fn save_task_if_tenant_aligned(
    store: &AppStore,
    mut task: TaskRecord,
) -> Result<TaskRecord, String> {
    if align_task_tenant_to_owner(&mut task) {
        task.updated_at = now_rfc3339();
        return store.save_task(task).await;
    }
    Ok(task)
}

#[cfg(test)]
mod tests {
    use crate::auth::CurrentUser;
    use crate::models::{TaskRecord, UserRole, PUBLIC_PROJECT_ID, TASK_PROFILE_DEFAULT};

    use super::{align_task_tenant_to_owner, resolve_task_tenant_id};

    fn normal_user() -> CurrentUser {
        CurrentUser {
            id: "agent-1".to_string(),
            username: "agent".to_string(),
            display_name: "Agent".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("real-user-1".to_string()),
            owner_username: Some("alice".to_string()),
            owner_display_name: Some("Alice".to_string()),
        }
    }

    fn admin_user() -> CurrentUser {
        CurrentUser {
            id: "admin-1".to_string(),
            username: "admin".to_string(),
            display_name: "Admin".to_string(),
            role: UserRole::Admin,
            owner_user_id: Some("admin-1".to_string()),
            owner_username: Some("admin".to_string()),
            owner_display_name: Some("Admin".to_string()),
        }
    }

    fn task_record(tenant_id: &str, owner_user_id: Option<&str>) -> TaskRecord {
        TaskRecord {
            id: "task-1".to_string(),
            title: "Task".to_string(),
            description: None,
            objective: "Do it".to_string(),
            input_payload: None,
            status: crate::models::TaskStatus::Draft,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: "task-task-1".to_string(),
            tenant_id: tenant_id.to_string(),
            subject_id: "subject".to_string(),
            project_id: PUBLIC_PROJECT_ID.to_string(),
            task_profile: TASK_PROFILE_DEFAULT.to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: None,
            owner_display_name: None,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: crate::models::TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: crate::models::TaskToolState::default(),
            mcp_config: crate::models::TaskMcpConfig::default(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            deleted_at: None,
        }
    }

    #[test]
    fn normal_user_tenant_is_locked_to_owner() {
        let user = normal_user();
        let tenant = resolve_task_tenant_id(
            Some("other-tenant".to_string()),
            Some(&user),
            "default-tenant",
        )
        .expect("tenant");
        assert_eq!(tenant, "real-user-1");
    }

    #[test]
    fn admin_can_request_tenant_but_defaults_to_self() {
        let admin = admin_user();
        let requested =
            resolve_task_tenant_id(Some("tenant-b".to_string()), Some(&admin), "default-tenant")
                .expect("tenant");
        assert_eq!(requested, "tenant-b");

        let defaulted =
            resolve_task_tenant_id(None, Some(&admin), "default-tenant").expect("tenant");
        assert_eq!(defaulted, "admin-1");
    }

    #[test]
    fn legacy_task_tenant_aligns_to_owner() {
        let mut task = task_record("legacy-tenant", Some("real-user-1"));
        assert!(align_task_tenant_to_owner(&mut task));
        assert_eq!(task.tenant_id, "real-user-1");
    }
}
