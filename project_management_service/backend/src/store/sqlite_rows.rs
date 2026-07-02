// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use crate::models::*;

pub(super) fn project_from_row(row: &SqliteRow) -> ProjectRecord {
    ProjectRecord {
        id: row.get("id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        name: row.get("name"),
        root_path: row.get("root_path"),
        git_url: row.get("git_url"),
        description: row.get("description"),
        status: ProjectStatus::from_db(row.get::<String, _>("status").as_str()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        archived_at: row.get("archived_at"),
    }
}

pub(super) fn project_profile_from_row(row: &SqliteRow) -> ProjectProfileRecord {
    ProjectProfileRecord {
        project_id: row.get("project_id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        background: row.get("background"),
        introduction: row.get("introduction"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub(super) fn requirement_from_row(row: &SqliteRow) -> RequirementRecord {
    RequirementRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        parent_requirement_id: row.get("parent_requirement_id"),
        requirement_type: row
            .get::<Option<String>, _>("requirement_type")
            .as_deref()
            .map(RequirementType::from_db)
            .unwrap_or_default(),
        title: row.get("title"),
        summary: row.get("summary"),
        detail: row.get("detail"),
        business_value: row.get("business_value"),
        acceptance_criteria: row.get("acceptance_criteria"),
        source: row.get("source"),
        priority: row.get("priority"),
        status: RequirementStatus::from_db(row.get::<String, _>("status").as_str()),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        assignee_user_id: row.get("assignee_user_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        archived_at: row.get("archived_at"),
    }
}

pub(super) fn requirement_dependency_from_row(row: &SqliteRow) -> RequirementDependencyRecord {
    RequirementDependencyRecord {
        requirement_id: row.get("requirement_id"),
        prerequisite_requirement_id: row.get("prerequisite_requirement_id"),
        relation_type: row.get("relation_type"),
        created_at: row.get("created_at"),
    }
}

pub(super) fn requirement_document_from_row(row: &SqliteRow) -> RequirementDocumentRecord {
    RequirementDocumentRecord {
        id: row.get("id"),
        requirement_id: row.get("requirement_id"),
        doc_type: row.get("doc_type"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        title: row.get("title"),
        format: row.get("format"),
        content: row.get("content"),
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub(super) fn work_item_from_row(row: &SqliteRow) -> ProjectWorkItemRecord {
    let tags_json = row.get::<String, _>("tags_json").trim().to_string();
    let tags = serde_json::from_str::<Vec<String>>(&tags_json).unwrap_or_default();
    let task_runner_enabled_tool_ids_json = row
        .try_get::<Option<String>, _>("task_runner_enabled_tool_ids_json")
        .ok()
        .flatten()
        .unwrap_or_else(|| "[]".to_string());
    let task_runner_enabled_tool_ids =
        serde_json::from_str::<Vec<String>>(&task_runner_enabled_tool_ids_json).unwrap_or_default();
    let task_runner_skill_ids_json = row
        .try_get::<Option<String>, _>("task_runner_skill_ids_json")
        .ok()
        .flatten()
        .unwrap_or_else(|| "[]".to_string());
    let task_runner_skill_ids =
        serde_json::from_str::<Vec<String>>(&task_runner_skill_ids_json).unwrap_or_default();
    ProjectWorkItemRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        requirement_id: row.get("requirement_id"),
        title: row.get("title"),
        description: row.get("description"),
        task_runner_default_model_config_id: row
            .try_get::<Option<String>, _>("task_runner_default_model_config_id")
            .ok()
            .flatten()
            .unwrap_or_default(),
        task_runner_enabled_tool_ids,
        task_runner_skill_ids,
        status: ProjectWorkItemStatus::from_db(row.get::<String, _>("status").as_str()),
        priority: row.get("priority"),
        assignee_user_id: row.get("assignee_user_id"),
        estimate_points: row.get("estimate_points"),
        due_at: row.get("due_at"),
        sort_order: row.get("sort_order"),
        tags,
        is_planning_task: row
            .try_get::<Option<bool>, _>("is_planning_task")
            .ok()
            .flatten()
            .unwrap_or(false),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        owner_user_id: row.get("owner_user_id"),
        owner_username: row.get("owner_username"),
        owner_display_name: row.get("owner_display_name"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        archived_at: row.get("archived_at"),
    }
}

pub(super) fn work_item_dependency_from_row(row: &SqliteRow) -> WorkItemDependencyRecord {
    WorkItemDependencyRecord {
        work_item_id: row.get("work_item_id"),
        prerequisite_work_item_id: row.get("prerequisite_work_item_id"),
        relation_type: row.get("relation_type"),
        created_at: row.get("created_at"),
    }
}

pub(super) fn task_runner_link_from_row(row: &SqliteRow) -> ProjectWorkItemTaskRunnerLinkRecord {
    ProjectWorkItemTaskRunnerLinkRecord {
        id: row.get("id"),
        work_item_id: row.get("work_item_id"),
        task_runner_task_id: row.get("task_runner_task_id"),
        task_runner_run_id: row.get("task_runner_run_id"),
        link_type: row.get("link_type"),
        source_session_id: row.try_get("source_session_id").ok().flatten(),
        source_user_message_id: row.try_get("source_user_message_id").ok().flatten(),
        task_runner_status: row.try_get("task_runner_status").ok().flatten(),
        last_callback_event: row.try_get("last_callback_event").ok().flatten(),
        last_callback_at: row.try_get("last_callback_at").ok().flatten(),
        last_error_message: row.try_get("last_error_message").ok().flatten(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
