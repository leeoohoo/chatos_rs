// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use crate::local_runtime::project_management::{
    CreateLocalRequirementInput, CreateLocalWorkItemInput,
};
use crate::LocalState;

pub(super) fn requirement_input() -> CreateLocalRequirementInput {
    CreateLocalRequirementInput {
        project_id: "project-task".to_string(),
        owner_user_id: "user-task".to_string(),
        parent_requirement_id: None,
        requirement_type: "requirement".to_string(),
        title: "Requirement".to_string(),
        summary: None,
        detail: None,
        business_value: None,
        acceptance_criteria: None,
        source: Some("test".to_string()),
        priority: 0,
        status: "approved".to_string(),
        assignee_user_id: None,
    }
}

pub(super) fn work_item_input(requirement_id: String) -> CreateLocalWorkItemInput {
    CreateLocalWorkItemInput {
        requirement_id,
        owner_user_id: "user-task".to_string(),
        title: "Work item".to_string(),
        description: Some("Do the work".to_string()),
        status: "todo".to_string(),
        priority: 1,
        assignee_user_id: None,
        estimate_points: None,
        due_at: None,
        sort_order: 0,
        tags: Vec::new(),
        is_planning_task: false,
    }
}

pub(super) fn local_state(root: &std::path::Path, provider_url: String) -> LocalState {
    serde_json::from_value(json!({
        "auth": {
            "cloud_base_url": "https://cloud.example.invalid",
            "user_service_base_url": "https://users.example.invalid",
            "access_token": "token", "device_name": "Test device",
            "user": { "id": "user-task", "username": "tester", "display_name": "Tester", "role": "user" }
        },
        "device_id": "device-task",
        "workspaces": [{
            "id": "workspace-task", "absolute_root": root.to_string_lossy(),
            "alias": "task-workspace", "fingerprint": "task-workspace-fingerprint"
        }],
        "model_configs": {
            "configs": [{
                "id": "model-task", "name": "Task model", "provider": "openai", "prompt_vendor": "gpt",
                "model": "gpt-test", "base_url": provider_url, "api_key": "secret",
                "enabled": true, "supports_images": false, "supports_reasoning": false,
                "supports_responses": true, "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            }],
            "settings": {}
        }
    }))
    .expect("build local state")
}
