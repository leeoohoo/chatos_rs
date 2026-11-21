use super::super::task_tools::CreateTaskItem;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::application) struct DependencyReduction {
    pub(in crate::application) submitted_edge_count: usize,
    pub(in crate::application) persisted_edge_count: usize,
    pub(in crate::application) dependencies: BTreeMap<String, Vec<String>>,
    pub(in crate::application) removed_edges: Vec<(String, String)>,
}

pub(in crate::application) fn reduce_client_ref_dependencies(
    tasks: &mut [CreateTaskItem],
) -> Result<DependencyReduction, String> {
    let node_ids = tasks
        .iter()
        .map(|task| task.client_ref.trim().to_string())
        .collect::<BTreeSet<_>>();
    let dependency_map = tasks
        .iter()
        .map(|task| {
            (
                task.client_ref.trim().to_string(),
                task.prerequisite_refs.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let submitted_edge_count = dependency_map
        .values()
        .map(|dependencies| {
            dependencies
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect::<BTreeSet<_>>()
                .len()
        })
        .sum();
    let reduction = transitive_reduce_prerequisite_map(&node_ids, &dependency_map)?;
    let mut removed_by_dependent = BTreeMap::<String, Vec<String>>::new();
    for (dependent_id, prerequisite_id) in &reduction.removed_edges {
        removed_by_dependent
            .entry(dependent_id.clone())
            .or_default()
            .push(prerequisite_id.clone());
    }
    for task in tasks {
        let client_ref = task.client_ref.trim();
        task.prerequisite_refs = reduction
            .dependencies
            .get(client_ref)
            .cloned()
            .unwrap_or_default();
        task.context_refs
            .extend(removed_by_dependent.remove(client_ref).unwrap_or_default());
        task.context_refs = task
            .context_refs
            .drain(..)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != client_ref)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }
    Ok(DependencyReduction {
        submitted_edge_count,
        persisted_edge_count: reduction.dependencies.values().map(Vec::len).sum(),
        dependencies: reduction.dependencies,
        removed_edges: reduction.removed_edges,
    })
}

fn transitive_reduce_prerequisite_map(
    node_ids: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<DependencyReduction, String> {
    if node_ids.iter().any(|node_id| node_id.trim().is_empty()) {
        return Err("dependency graph node id cannot be empty".to_string());
    }
    let mut normalized = node_ids
        .iter()
        .map(|node_id| (node_id.clone(), Vec::<String>::new()))
        .collect::<BTreeMap<_, _>>();
    for (dependent_id, prerequisite_ids) in dependency_map {
        if !node_ids.contains(dependent_id) {
            return Err(format!(
                "dependency graph contains unknown task: {dependent_id}"
            ));
        }
        let dependencies = prerequisite_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        for prerequisite_id in &dependencies {
            if prerequisite_id == dependent_id {
                return Err(format!("task cannot depend on itself: {dependent_id}"));
            }
            if !node_ids.contains(prerequisite_id) {
                return Err(format!(
                    "task {dependent_id} contains unknown prerequisite: {prerequisite_id}"
                ));
            }
        }
        normalized.insert(dependent_id.clone(), dependencies);
    }
    ensure_prerequisite_map_acyclic(node_ids, &normalized)?;
    let mut reduced = normalized.clone();
    let mut removed_edges = Vec::new();
    for (dependent_id, prerequisite_ids) in &normalized {
        for prerequisite_id in prerequisite_ids {
            if prerequisite_reachable_without_edge(
                dependent_id,
                prerequisite_id,
                &normalized,
                (dependent_id, prerequisite_id),
            ) {
                if let Some(dependencies) = reduced.get_mut(dependent_id) {
                    dependencies.retain(|value| value != prerequisite_id);
                }
                removed_edges.push((dependent_id.clone(), prerequisite_id.clone()));
            }
        }
    }
    Ok(DependencyReduction {
        submitted_edge_count: 0,
        persisted_edge_count: 0,
        dependencies: reduced,
        removed_edges,
    })
}

fn ensure_prerequisite_map_acyclic(
    node_ids: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let mut pending = node_ids.clone();
    let mut resolved = BTreeSet::new();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .filter(|node_id| {
                dependency_map
                    .get(node_id.as_str())
                    .into_iter()
                    .flatten()
                    .all(|prerequisite_id| resolved.contains(prerequisite_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err("task dependency graph contains a cycle".to_string());
        }
        for node_id in ready {
            pending.remove(node_id.as_str());
            resolved.insert(node_id);
        }
    }
    Ok(())
}

fn prerequisite_reachable_without_edge(
    start: &str,
    target: &str,
    dependency_map: &BTreeMap<String, Vec<String>>,
    excluded_edge: (&str, &str),
) -> bool {
    let mut pending = vec![start.to_string()];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        for prerequisite_id in dependency_map.get(current.as_str()).into_iter().flatten() {
            if current == excluded_edge.0 && prerequisite_id == excluded_edge.1 {
                continue;
            }
            if prerequisite_id == target {
                return true;
            }
            pending.push(prerequisite_id.clone());
        }
    }
    false
}

pub(in crate::application) fn attach_dependency_context_payload(
    input_payload: Value,
    client_ref: &str,
    context_refs: &[String],
) -> Value {
    let mut payload = match input_payload {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        value => {
            let mut map = serde_json::Map::new();
            map.insert("input".to_string(), value);
            map
        }
    };
    payload.insert(
        "execution_client_ref".to_string(),
        Value::String(client_ref.to_string()),
    );
    payload.insert(
        "dependency_context_refs".to_string(),
        Value::Array(context_refs.iter().cloned().map(Value::String).collect()),
    );
    Value::Object(payload)
}
