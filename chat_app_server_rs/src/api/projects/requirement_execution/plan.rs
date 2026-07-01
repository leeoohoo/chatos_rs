// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::errors::HandlerError;
use super::status::is_done_status;
use super::types::{RequirementPlanItem, WorkItemPlanItem};
use super::values::{value_i64, value_string, value_string_vec};

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
                task_runner_default_model_config_id: value_string(
                    &value,
                    "task_runner_default_model_config_id",
                )
                .or_else(|| value_string(&value, "taskRunnerDefaultModelConfigId"))
                .unwrap_or_default(),
                task_runner_enabled_tool_ids: value_string_vec(
                    &value,
                    "task_runner_enabled_tool_ids",
                )
                .or_else(|| value_string_vec(&value, "taskRunnerEnabledToolIds"))
                .unwrap_or_default(),
                task_runner_skill_ids: value_string_vec(&value, "task_runner_skill_ids")
                    .or_else(|| value_string_vec(&value, "taskRunnerSkillIds"))
                    .unwrap_or_default(),
                status: value_string(&value, "status")
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                priority: value_i64(&value, "priority")
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or_default(),
                tags: value_string_vec(&value, "tags").unwrap_or_default(),
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
    let mut scope = BTreeSet::from([root_id.to_string()]);
    expand_requirement_descendants(items, &mut scope);

    let mut downstream_scope = scope.clone();
    loop {
        let before = downstream_scope.len();
        for (requirement_id, prerequisite_ids) in dependency_map {
            if prerequisite_ids
                .iter()
                .any(|prerequisite_id| downstream_scope.contains(prerequisite_id))
            {
                downstream_scope.insert(requirement_id.clone());
            }
        }
        expand_requirement_descendants(items, &mut downstream_scope);
        if downstream_scope.len() == before {
            break;
        }
    }
    scope = downstream_scope;

    let status_by_id = items
        .iter()
        .map(|item| (item.id.as_str(), item.status.as_str()))
        .collect::<BTreeMap<_, _>>();
    loop {
        let before = scope.len();
        let current_ids = scope.iter().cloned().collect::<Vec<_>>();
        for requirement_id in current_ids {
            for prerequisite_id in dependency_map
                .get(requirement_id.as_str())
                .into_iter()
                .flatten()
            {
                if scope.contains(prerequisite_id.as_str()) {
                    continue;
                }
                match status_by_id.get(prerequisite_id.as_str()) {
                    Some(status) if is_done_status(status) => continue,
                    Some(_) => {
                        scope.insert(prerequisite_id.clone());
                    }
                    None => {}
                }
            }
        }
        expand_requirement_descendants(items, &mut scope);
        if include_prerequisite_dependents {
            expand_requirement_dependents(dependency_map, &mut scope);
            expand_requirement_descendants(items, &mut scope);
        }
        if scope.len() == before {
            break;
        }
    }
    scope
}

fn expand_requirement_descendants(items: &[RequirementPlanItem], scope: &mut BTreeSet<String>) {
    loop {
        let before = scope.len();
        for item in items {
            if item
                .parent_requirement_id
                .as_deref()
                .is_some_and(|parent_id| scope.contains(parent_id))
            {
                scope.insert(item.id.clone());
            }
        }
        if scope.len() == before {
            break;
        }
    }
}

fn expand_requirement_dependents(
    dependency_map: &BTreeMap<String, Vec<String>>,
    scope: &mut BTreeSet<String>,
) {
    loop {
        let before = scope.len();
        for (requirement_id, prerequisite_ids) in dependency_map {
            if prerequisite_ids
                .iter()
                .any(|prerequisite_id| scope.contains(prerequisite_id))
            {
                scope.insert(requirement_id.clone());
            }
        }
        if scope.len() == before {
            break;
        }
    }
}

pub(in crate::api::projects) fn add_requirement_work_item_dependencies(
    dependency_map: &mut BTreeMap<String, Vec<String>>,
    work_items: &[WorkItemPlanItem],
    requirement_dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_scope: &BTreeSet<String>,
) {
    for work_item in work_items {
        for prerequisite_requirement_id in requirement_dependency_map
            .get(work_item.requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|requirement_id| requirement_scope.contains(requirement_id.as_str()))
        {
            for prerequisite_item in work_items.iter().filter(|candidate| {
                candidate.requirement_id == *prerequisite_requirement_id
                    && candidate.id != work_item.id
            }) {
                dependency_map
                    .entry(work_item.id.clone())
                    .or_default()
                    .push(prerequisite_item.id.clone());
            }
        }
    }
    for deps in dependency_map.values_mut() {
        deps.sort();
        deps.dedup();
    }
}

pub(in crate::api::projects) fn validate_requirement_prerequisites(
    items: &[RequirementPlanItem],
    requirement_scope: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<(), HandlerError> {
    let by_id = items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut blockers = Vec::new();
    for requirement_id in requirement_scope {
        let requirement_title = by_id
            .get(requirement_id.as_str())
            .map(|item| item.title.as_str())
            .unwrap_or(requirement_id.as_str());
        for prerequisite_id in dependency_map
            .get(requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|prerequisite_id| !requirement_scope.contains(prerequisite_id.as_str()))
        {
            match by_id.get(prerequisite_id.as_str()) {
                Some(prerequisite) if is_done_status(prerequisite.status.as_str()) => {}
                Some(prerequisite) => blockers.push(format!(
                    "{} 的前置需求未完成：{}（{}）",
                    requirement_title, prerequisite.title, prerequisite.status
                )),
                None => blockers.push(format!(
                    "{} 的前置需求不存在或不可见：{}",
                    requirement_title, prerequisite_id
                )),
            }
        }
    }
    if blockers.is_empty() {
        return Ok(());
    }
    blockers.sort();
    blockers.dedup();
    Err(HandlerError::bad_request(format!(
        "存在未完成的前置需求，无法执行：{}",
        blockers.join("；")
    )))
}

pub(in crate::api::projects) fn topological_work_item_order(
    work_items: &[WorkItemPlanItem],
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, HandlerError> {
    let work_item_ids = work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let mut pending = work_item_ids.clone();
    let mut ready_done = BTreeSet::new();
    let mut order = Vec::new();

    while !pending.is_empty() {
        let ready_ids = pending
            .iter()
            .filter(|work_item_id| {
                dependency_map
                    .get(work_item_id.as_str())
                    .into_iter()
                    .flatten()
                    .filter(|dep_id| work_item_ids.contains(dep_id.as_str()))
                    .all(|dep_id| ready_done.contains(dep_id.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready_ids.is_empty() {
            return Err(HandlerError::bad_request(
                "项目任务存在循环前置关系，无法执行",
            ));
        }
        for work_item_id in ready_ids {
            pending.remove(work_item_id.as_str());
            ready_done.insert(work_item_id.clone());
            order.push(work_item_id);
        }
    }

    Ok(order)
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

#[cfg(test)]
mod tests {
    use super::*;

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
