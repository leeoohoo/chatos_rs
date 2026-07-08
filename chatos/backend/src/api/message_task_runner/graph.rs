// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use super::normalize_text;

#[derive(Debug, Clone)]
struct GraphNodeEdgeSource {
    id: String,
    prerequisite_task_ids: Vec<String>,
}

fn graph_task_id(node: &Value) -> Option<String> {
    normalize_text(node.get("task")?.get("id")?.as_str())
}

fn task_id(task: &Value) -> Option<String> {
    normalize_text(task.get("id")?.as_str())
}

fn task_is_top_level(task: &Value) -> bool {
    normalize_text(task.get("parent_task_id").and_then(Value::as_str)).is_none()
}

fn graph_node_is_top_level(node: &Value) -> bool {
    node.get("task").is_none_or(task_is_top_level)
}

fn task_prerequisite_ids(task: &Value) -> Vec<String> {
    task.get("prerequisite_task_ids")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| normalize_text(item.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn graph_task_prerequisite_ids(node: &Value) -> Vec<String> {
    node.get("task")
        .map(task_prerequisite_ids)
        .unwrap_or_default()
}

fn task_prerequisite_summaries(task: &Value) -> Vec<Value> {
    task.get("prerequisite_tasks")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| task_id(item).is_some())
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn short_task_id(value: &str) -> String {
    if value.chars().count() > 16 {
        let prefix = value.chars().take(8).collect::<String>();
        let suffix = value
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        format!("{prefix}...{suffix}")
    } else {
        value.to_string()
    }
}

fn normalize_graph_task_shape(mut task: Value, task_id: &str, child_task: Option<&Value>) -> Value {
    if !task.is_object() {
        task = json!({});
    }
    let child_source_session_id =
        child_task.and_then(|task| normalize_text(task.get("source_session_id")?.as_str()));
    let child_source_turn_id =
        child_task.and_then(|task| normalize_text(task.get("source_turn_id")?.as_str()));
    let child_source_user_message_id =
        child_task.and_then(|task| normalize_text(task.get("source_user_message_id")?.as_str()));

    let Some(task_object) = task.as_object_mut() else {
        return json!({
            "id": task_id,
            "title": format!("前置任务 {}", short_task_id(task_id)),
            "prerequisite_task_ids": [],
            "prerequisite_tasks": [],
        });
    };
    task_object.insert("id".to_string(), json!(task_id));
    if normalize_text(task_object.get("title").and_then(Value::as_str)).is_none() {
        task_object.insert(
            "title".to_string(),
            json!(format!("前置任务 {}", short_task_id(task_id))),
        );
    }
    if !task_object
        .get("prerequisite_task_ids")
        .is_some_and(Value::is_array)
    {
        task_object.insert("prerequisite_task_ids".to_string(), json!([]));
    }
    if !task_object
        .get("prerequisite_tasks")
        .is_some_and(Value::is_array)
    {
        task_object.insert("prerequisite_tasks".to_string(), json!([]));
    }
    if normalize_text(task_object.get("source_session_id").and_then(Value::as_str)).is_none() {
        if let Some(value) = child_source_session_id {
            task_object.insert("source_session_id".to_string(), json!(value));
        }
    }
    if normalize_text(task_object.get("source_turn_id").and_then(Value::as_str)).is_none() {
        if let Some(value) = child_source_turn_id {
            task_object.insert("source_turn_id".to_string(), json!(value));
        }
    }
    if normalize_text(
        task_object
            .get("source_user_message_id")
            .and_then(Value::as_str),
    )
    .is_none()
    {
        if let Some(value) = child_source_user_message_id {
            task_object.insert("source_user_message_id".to_string(), json!(value));
        }
    }
    task
}

fn supplement_missing_graph_prerequisite_nodes(
    nodes: &mut Vec<Value>,
    supplemental_tasks: &[Value],
    excluded_task_ids: &HashSet<String>,
) {
    let supplemental_task_by_id = supplemental_tasks
        .iter()
        .filter(|task| task_is_top_level(task))
        .filter_map(|task| task_id(task).map(|id| (id, task.clone())))
        .collect::<HashMap<_, _>>();
    let mut known_node_ids = nodes
        .iter()
        .filter_map(graph_task_id)
        .collect::<HashSet<_>>();
    let mut summary_by_id = HashMap::<String, Value>::new();

    let mut index = 0;
    while index < nodes.len() {
        let Some(child_task) = nodes.get(index).and_then(|node| node.get("task")).cloned() else {
            index += 1;
            continue;
        };
        for summary in task_prerequisite_summaries(&child_task) {
            if let Some(summary_id) = task_id(&summary) {
                if excluded_task_ids.contains(summary_id.as_str()) {
                    continue;
                }
                summary_by_id.entry(summary_id).or_insert(summary);
            }
        }

        for prerequisite_task_id in task_prerequisite_ids(&child_task) {
            if excluded_task_ids.contains(prerequisite_task_id.as_str()) {
                continue;
            }
            if known_node_ids.contains(prerequisite_task_id.as_str()) {
                continue;
            }
            let task = supplemental_task_by_id
                .get(prerequisite_task_id.as_str())
                .cloned()
                .or_else(|| summary_by_id.get(prerequisite_task_id.as_str()).cloned())
                .unwrap_or_else(|| json!({ "id": prerequisite_task_id }));
            let normalized_task =
                normalize_graph_task_shape(task, prerequisite_task_id.as_str(), Some(&child_task));
            nodes.push(json!({
                "depth": 0,
                "is_root": false,
                "is_current_message": false,
                "task": normalized_task,
            }));
            known_node_ids.insert(prerequisite_task_id);
        }
        index += 1;
    }
}

fn push_normalized_graph_edge(
    edge_ids: &mut HashSet<String>,
    normalized_edge_sources: &mut Vec<(String, String, String)>,
    known_node_ids: &HashSet<String>,
    source: Option<&str>,
    target: Option<&str>,
    kind: Option<&str>,
) {
    let Some(source) = normalize_text(source) else {
        return;
    };
    let Some(target) = normalize_text(target) else {
        return;
    };
    if source == target {
        return;
    }
    if !known_node_ids.contains(source.as_str()) || !known_node_ids.contains(target.as_str()) {
        return;
    }
    let edge_id = format!("{source}->{target}");
    if !edge_ids.insert(edge_id) {
        return;
    }
    normalized_edge_sources.push((
        source,
        target,
        normalize_text(kind).unwrap_or_else(|| "prerequisite".to_string()),
    ));
}

#[cfg(test)]
fn normalize_message_task_graph_payload_edges(payload: Value) -> Value {
    normalize_message_task_graph_payload_edges_with_tasks(payload, &[])
}

pub(super) fn normalize_message_task_graph_payload_edges_with_tasks(
    mut payload: Value,
    supplemental_tasks: &[Value],
) -> Value {
    let Some(raw_nodes) = payload.get("nodes").and_then(Value::as_array) else {
        return payload;
    };
    let mut excluded_task_ids = raw_nodes
        .iter()
        .filter(|node| !graph_node_is_top_level(node))
        .filter_map(graph_task_id)
        .collect::<HashSet<_>>();
    excluded_task_ids.extend(
        supplemental_tasks
            .iter()
            .filter(|task| !task_is_top_level(task))
            .filter_map(task_id),
    );
    let mut nodes = raw_nodes
        .iter()
        .filter(|node| graph_node_is_top_level(node))
        .cloned()
        .collect::<Vec<_>>();
    supplement_missing_graph_prerequisite_nodes(&mut nodes, supplemental_tasks, &excluded_task_ids);
    let node_sources = nodes
        .iter()
        .filter_map(|node| {
            graph_task_id(node).map(|id| GraphNodeEdgeSource {
                id,
                prerequisite_task_ids: graph_task_prerequisite_ids(node)
                    .into_iter()
                    .filter(|task_id| !excluded_task_ids.contains(task_id.as_str()))
                    .collect(),
            })
        })
        .collect::<Vec<_>>();
    let mut edge_ids = HashSet::<String>::new();
    let mut normalized_edge_sources = Vec::<(String, String, String)>::new();
    let known_node_ids = node_sources
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();

    for node in &node_sources {
        for prerequisite_task_id in &node.prerequisite_task_ids {
            push_normalized_graph_edge(
                &mut edge_ids,
                &mut normalized_edge_sources,
                &known_node_ids,
                Some(prerequisite_task_id.as_str()),
                Some(node.id.as_str()),
                Some("prerequisite"),
            );
        }
    }
    if normalized_edge_sources.is_empty() {
        if let Some(edges) = payload.get("edges").and_then(Value::as_array) {
            for edge in edges {
                push_normalized_graph_edge(
                    &mut edge_ids,
                    &mut normalized_edge_sources,
                    &known_node_ids,
                    edge.get("source").and_then(Value::as_str),
                    edge.get("target").and_then(Value::as_str),
                    edge.get("kind").and_then(Value::as_str),
                );
            }
        }
    }

    let mut depth_by_id = known_node_ids
        .iter()
        .map(|task_id| (task_id.clone(), 0_i64))
        .collect::<HashMap<_, _>>();
    for _ in 0..known_node_ids.len() {
        let mut changed = false;
        for (source, target, _) in &normalized_edge_sources {
            let target_depth = depth_by_id.get(target.as_str()).copied().unwrap_or(0);
            let source_depth = depth_by_id.get(source.as_str()).copied().unwrap_or(0);
            let next_source_depth = target_depth + 1;
            if next_source_depth > source_depth {
                depth_by_id.insert(source.clone(), next_source_depth);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let normalized_edges = normalized_edge_sources
        .into_iter()
        .map(|(source, target, kind)| {
            json!({
                "id": format!("{source}->{target}"),
                "source": source,
                "target": target,
                "kind": kind,
            })
        })
        .collect::<Vec<_>>();
    for node in &mut nodes {
        let Some(task_id) = graph_task_id(node) else {
            continue;
        };
        let Some(depth) = depth_by_id.get(task_id.as_str()).copied() else {
            continue;
        };
        if let Some(node_object) = node.as_object_mut() {
            node_object.insert("depth".to_string(), json!(depth));
        }
    }
    if let Some(payload_object) = payload.as_object_mut() {
        let normalized_root_task_ids = payload_object
            .get("root_task_ids")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| normalize_text(item.as_str()))
                    .filter(|task_id| known_node_ids.contains(task_id.as_str()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        payload_object.insert("root_task_ids".to_string(), json!(normalized_root_task_ids));
        payload_object.insert("nodes".to_string(), Value::Array(nodes));
        payload_object.insert("edges".to_string(), Value::Array(normalized_edges));
    }
    payload
}

#[cfg(test)]
#[path = "graph/tests.rs"]
mod tests;
