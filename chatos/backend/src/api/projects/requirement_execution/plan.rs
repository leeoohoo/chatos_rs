// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::errors::HandlerError;
use super::types::{RequirementPlanItem, WorkItemPlanItem};
use super::values::{value_bool, value_i64, value_string, value_string_vec};

pub(in crate::api::projects) fn project_plan_array(
    plan: &Value,
    snake_key: &str,
    camel_key: &str,
) -> Vec<Value> {
    plan.get(snake_key)
        .or_else(|| plan.get(camel_key))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub(in crate::api::projects) fn project_plan_value(
    plan: &Value,
    snake_key: &str,
    camel_key: &str,
) -> Value {
    plan.get(snake_key)
        .or_else(|| plan.get(camel_key))
        .cloned()
        .unwrap_or_else(|| json!({}))
}

pub(in crate::api::projects) fn parse_requirements(values: Vec<Value>) -> Vec<RequirementPlanItem> {
    values
        .into_iter()
        .filter_map(|value| {
            Some(RequirementPlanItem {
                id: value_string(&value, "id")?,
                title: value_string(&value, "title").unwrap_or_else(|| "未命名需求".to_string()),
                status: value_string(&value, "status")
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                parent_requirement_id: value_string(&value, "parent_requirement_id")
                    .or_else(|| value_string(&value, "parentRequirementId")),
            })
        })
        .collect()
}

pub(in crate::api::projects) fn parse_work_items(values: Vec<Value>) -> Vec<WorkItemPlanItem> {
    values
        .into_iter()
        .filter_map(|value| {
            Some(WorkItemPlanItem {
                id: value_string(&value, "id")?,
                requirement_id: value_string(&value, "requirement_id")
                    .or_else(|| value_string(&value, "requirementId"))?,
                title: value_string(&value, "title")
                    .unwrap_or_else(|| "未命名项目任务".to_string()),
                description: value_string(&value, "description"),
                status: value_string(&value, "status")
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                priority: value_i64(&value, "priority").unwrap_or_default(),
                tags: value_string_vec(&value, "tags").unwrap_or_default(),
                is_planning_task: value_bool(&value, "is_planning_task")
                    .or_else(|| value_bool(&value, "isPlanningTask"))
                    .unwrap_or(false),
            })
        })
        .collect()
}

pub(in crate::api::projects) fn collect_requirement_execution_scope(
    items: &[RequirementPlanItem],
    root_id: &str,
    dependency_map: &BTreeMap<String, Vec<String>>,
    include_prerequisite_dependents: bool,
) -> BTreeSet<String> {
    chatos_project_execution::collect_requirement_execution_scope(
        items,
        root_id,
        dependency_map,
        include_prerequisite_dependents,
    )
}

pub(in crate::api::projects) fn add_requirement_work_item_dependencies(
    dependency_map: &mut BTreeMap<String, Vec<String>>,
    work_items: &[WorkItemPlanItem],
    requirement_dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_scope: &BTreeSet<String>,
) {
    chatos_project_execution::add_requirement_work_item_dependencies(
        dependency_map,
        work_items,
        requirement_dependency_map,
        requirement_scope,
    );
}

pub(in crate::api::projects) fn validate_requirement_prerequisites(
    items: &[RequirementPlanItem],
    requirement_scope: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<(), HandlerError> {
    chatos_project_execution::validate_requirement_prerequisites(
        items,
        requirement_scope,
        dependency_map,
    )
    .map_err(HandlerError::bad_request)
}

pub(in crate::api::projects) fn topological_work_item_order(
    work_items: &[WorkItemPlanItem],
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, HandlerError> {
    chatos_project_execution::topological_work_item_order(work_items, dependency_map)
        .map_err(HandlerError::bad_request)
}

pub(in crate::api::projects) fn requirement_dependency_map(
    graph: &Value,
) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Some(edges) = graph.get("edges").and_then(Value::as_array) else {
        return out;
    };
    for edge in edges {
        let Some(from) = value_string(edge, "from") else {
            continue;
        };
        let Some(to) = value_string(edge, "to") else {
            continue;
        };
        let Some(prereq_id) = from.strip_prefix("requirement:") else {
            continue;
        };
        let Some(requirement_id) = to.strip_prefix("requirement:") else {
            continue;
        };
        out.entry(requirement_id.to_string())
            .or_default()
            .push(prereq_id.to_string());
    }
    for deps in out.values_mut() {
        deps.sort();
        deps.dedup();
    }
    out
}

pub(in crate::api::projects) fn work_item_dependency_map(
    graph: &Value,
) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Some(edges) = graph.get("edges").and_then(Value::as_array) else {
        return out;
    };
    for edge in edges {
        let Some(from) = value_string(edge, "from") else {
            continue;
        };
        let Some(to) = value_string(edge, "to") else {
            continue;
        };
        let Some(prereq_id) = from.strip_prefix("work_item:") else {
            continue;
        };
        let Some(work_item_id) = to.strip_prefix("work_item:") else {
            continue;
        };
        out.entry(work_item_id.to_string())
            .or_default()
            .push(prereq_id.to_string());
    }
    for deps in out.values_mut() {
        deps.sort();
        deps.dedup();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn requirement(id: &str, parent_requirement_id: Option<&str>) -> RequirementPlanItem {
        requirement_with_status(id, parent_requirement_id, "approved")
    }

    fn requirement_with_status(
        id: &str,
        parent_requirement_id: Option<&str>,
        status: &str,
    ) -> RequirementPlanItem {
        RequirementPlanItem {
            id: id.to_string(),
            title: id.to_string(),
            status: status.to_string(),
            parent_requirement_id: parent_requirement_id.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn execution_scope_includes_downstream_and_required_prerequisites() {
        let requirements = vec![
            requirement("parent", None),
            requirement("child", Some("parent")),
            requirement("grandchild", Some("child")),
            requirement("dependent", None),
            requirement("dependent-child", Some("dependent")),
            requirement("after-dependent", None),
            requirement("sibling", Some("parent")),
            requirement("prerequisite", None),
            requirement("unrelated-dependent", None),
        ];
        let dependency_map = BTreeMap::from([
            (
                "dependent".to_string(),
                vec!["child".to_string(), "prerequisite".to_string()],
            ),
            ("after-dependent".to_string(), vec!["dependent".to_string()]),
            (
                "unrelated-dependent".to_string(),
                vec!["prerequisite".to_string()],
            ),
        ]);

        let scope =
            collect_requirement_execution_scope(&requirements, "child", &dependency_map, false);

        assert!(scope.contains("child"));
        assert!(scope.contains("grandchild"));
        assert!(scope.contains("dependent"));
        assert!(scope.contains("dependent-child"));
        assert!(scope.contains("after-dependent"));
        assert!(scope.contains("prerequisite"));
        assert!(!scope.contains("unrelated-dependent"));
        assert!(!scope.contains("parent"));
        assert!(!scope.contains("sibling"));
    }

    #[test]
    fn execution_scope_can_include_required_prerequisite_dependents() {
        let requirements = vec![
            requirement("child", None),
            requirement("dependent", None),
            requirement("after-dependent", None),
            requirement("prerequisite", None),
            requirement("unrelated-dependent", None),
        ];
        let dependency_map = BTreeMap::from([
            (
                "dependent".to_string(),
                vec!["child".to_string(), "prerequisite".to_string()],
            ),
            ("after-dependent".to_string(), vec!["dependent".to_string()]),
            (
                "unrelated-dependent".to_string(),
                vec!["prerequisite".to_string()],
            ),
        ]);

        let scope =
            collect_requirement_execution_scope(&requirements, "child", &dependency_map, true);

        assert!(scope.contains("child"));
        assert!(scope.contains("dependent"));
        assert!(scope.contains("after-dependent"));
        assert!(scope.contains("prerequisite"));
        assert!(scope.contains("unrelated-dependent"));
    }

    #[test]
    fn execution_scope_skips_completed_external_prerequisites() {
        let requirements = vec![
            requirement("root", None),
            requirement("dependent", None),
            requirement_with_status("completed-prerequisite", None, "done"),
        ];
        let dependency_map = BTreeMap::from([(
            "dependent".to_string(),
            vec!["root".to_string(), "completed-prerequisite".to_string()],
        )]);

        let scope =
            collect_requirement_execution_scope(&requirements, "root", &dependency_map, false);

        assert!(scope.contains("root"));
        assert!(scope.contains("dependent"));
        assert!(!scope.contains("completed-prerequisite"));
    }

    #[test]
    fn parse_work_items_reads_planning_task_flags() {
        let items = parse_work_items(vec![
            json!({
                "id": "task-snake",
                "requirement_id": "req-1",
                "title": "继续拆解方案",
                "is_planning_task": true
            }),
            json!({
                "id": "task-camel",
                "requirementId": "req-1",
                "title": "补充技术计划",
                "isPlanningTask": true
            }),
            json!({
                "id": "task-default",
                "requirement_id": "req-1",
                "title": "实现接口"
            }),
        ]);

        assert_eq!(items.len(), 3);
        assert!(items[0].is_planning_task);
        assert!(items[1].is_planning_task);
        assert!(!items[2].is_planning_task);
    }
}
