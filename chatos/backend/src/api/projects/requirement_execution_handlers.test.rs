// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_request_accepts_selected_model_in_snake_case() {
        let request: ExecuteRequirementRequest = serde_json::from_value(serde_json::json!({
            "model_config_id": "model-selected",
            "include_prerequisite_dependents": true
        }))
        .expect("request should deserialize");

        assert_eq!(request.model_config_id.as_deref(), Some("model-selected"));
        assert!(request.include_prerequisite_dependents);
    }

    #[test]
    fn execute_request_accepts_selected_model_in_camel_case() {
        let request: ExecuteRequirementRequest = serde_json::from_value(serde_json::json!({
            "modelConfigId": "model-selected"
        }))
        .expect("request should deserialize");

        assert_eq!(request.model_config_id.as_deref(), Some("model-selected"));
    }

    #[test]
    fn planner_prompt_requires_task_creation_for_planning_work_items() {
        let requirement = RequirementPlanItem {
            id: "requirement-1".to_string(),
            title: "Plan migration".to_string(),
            status: "approved".to_string(),
            parent_requirement_id: None,
        };
        let work_item = WorkItemPlanItem {
            id: "project-task-1".to_string(),
            requirement_id: requirement.id.clone(),
            title: "Create migration plan".to_string(),
            description: Some("Structured planning details".to_string()),
            status: "ready".to_string(),
            priority: 1,
            tags: vec!["planning".to_string()],
            is_planning_task: true,
        };
        let prompt = build_requirement_execution_planner_prompt(
            "project-1",
            &requirement,
            std::slice::from_ref(&requirement),
            &BTreeSet::from([requirement.id.clone()]),
            std::slice::from_ref(&work_item),
            std::slice::from_ref(&work_item.id),
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some("model-selected"),
        );

        assert!(prompt.contains("must_call_tool"));
        assert!(prompt.contains("create_project_execution_tasks"));
        assert!(prompt.contains("project-task-1"));
        assert!(prompt.contains("model-selected"));
        assert!(prompt.contains("requires_execution=false"));
        assert!(prompt.contains("不是规划完整性复核"));
    }

    #[test]
    fn requirement_execution_user_message_is_concise_and_hides_internal_contract() {
        let requirement = RequirementPlanItem {
            id: "requirement-1".to_string(),
            title: "JDK 21 upgrade".to_string(),
            status: "approved".to_string(),
            parent_requirement_id: None,
        };
        let work_items = (1..=4)
            .map(|index| WorkItemPlanItem {
                id: format!("project-task-{index}"),
                requirement_id: requirement.id.clone(),
                title: format!("Migration task {index}"),
                description: None,
                status: "ready".to_string(),
                priority: index,
                tags: Vec::new(),
                is_planning_task: false,
            })
            .collect::<Vec<_>>();

        let content = build_requirement_execution_user_message(&requirement, &work_items);

        assert!(content.contains("执行需求「JDK 21 upgrade」的 4 个关联任务"));
        assert!(content.contains("Migration task 1"));
        assert!(content.contains("另有 1 个关联任务"));
        assert!(!content.contains("create_project_execution_tasks"));
        assert!(!content.contains("execution_contract"));
        assert!(!content.contains("project-task-1"));
    }
}
