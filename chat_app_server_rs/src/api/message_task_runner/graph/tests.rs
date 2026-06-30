use super::*;
use serde_json::{json, Value};

fn normalized_edges(payload: &Value) -> Vec<Value> {
    payload
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn node_depth(payload: &Value, task_id: &str) -> Option<i64> {
    payload
        .get("nodes")
        .and_then(Value::as_array)?
        .iter()
        .find(|node| graph_task_id(node).as_deref() == Some(task_id))?
        .get("depth")
        .and_then(Value::as_i64)
}

fn has_node(payload: &Value, task_id: &str) -> bool {
    payload
        .get("nodes")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes
                .iter()
                .any(|node| graph_task_id(node).as_deref() == Some(task_id))
        })
}

#[test]
fn normalize_graph_edges_keeps_parallel_prerequisites_parallel() {
    let payload = json!({
        "root_task_ids": ["current"],
        "nodes": [
            {
                "depth": 0,
                "is_root": true,
                "is_current_message": true,
                "task": {
                    "id": "current",
                    "title": "当前任务",
                    "prerequisite_task_ids": ["prereq-a", "prereq-b"]
                }
            },
            {
                "depth": 1,
                "is_root": false,
                "is_current_message": false,
                "task": {
                    "id": "prereq-a",
                    "title": "前置 A",
                    "prerequisite_task_ids": []
                }
            },
            {
                "depth": 1,
                "is_root": false,
                "is_current_message": false,
                "task": {
                    "id": "prereq-b",
                    "title": "前置 B",
                    "prerequisite_task_ids": []
                }
            }
        ],
        "edges": [
            {
                "id": "prereq-a->prereq-b",
                "source": "prereq-a",
                "target": "prereq-b",
                "kind": "prerequisite"
            },
            {
                "id": "prereq-b->current",
                "source": "prereq-b",
                "target": "current",
                "kind": "prerequisite"
            }
        ]
    });

    let normalized = normalize_message_task_graph_payload_edges(payload);

    assert_eq!(
        normalized_edges(&normalized),
        vec![
            json!({
                "id": "prereq-a->current",
                "source": "prereq-a",
                "target": "current",
                "kind": "prerequisite"
            }),
            json!({
                "id": "prereq-b->current",
                "source": "prereq-b",
                "target": "current",
                "kind": "prerequisite"
            })
        ]
    );
    assert_eq!(node_depth(&normalized, "current"), Some(0));
    assert_eq!(node_depth(&normalized, "prereq-a"), Some(1));
    assert_eq!(node_depth(&normalized, "prereq-b"), Some(1));
}

#[test]
fn normalize_graph_edges_keeps_declared_serial_prerequisite_edges() {
    let payload = json!({
        "root_task_ids": ["current"],
        "nodes": [
            {
                "depth": 0,
                "is_root": true,
                "is_current_message": true,
                "task": {
                    "id": "current",
                    "title": "当前任务",
                    "prerequisite_task_ids": ["prereq-a", "prereq-b"]
                }
            },
            {
                "depth": 1,
                "is_root": false,
                "is_current_message": false,
                "task": {
                    "id": "prereq-a",
                    "title": "前置 A",
                    "prerequisite_task_ids": []
                }
            },
            {
                "depth": 1,
                "is_root": false,
                "is_current_message": false,
                "task": {
                    "id": "prereq-b",
                    "title": "前置 B",
                    "prerequisite_task_ids": ["prereq-a"]
                }
            }
        ],
        "edges": [
            {
                "id": "prereq-b->current",
                "source": "prereq-b",
                "target": "current",
                "kind": "prerequisite"
            }
        ]
    });

    let normalized = normalize_message_task_graph_payload_edges(payload);

    assert_eq!(
        normalized_edges(&normalized),
        vec![
            json!({
                "id": "prereq-a->current",
                "source": "prereq-a",
                "target": "current",
                "kind": "prerequisite"
            }),
            json!({
                "id": "prereq-b->current",
                "source": "prereq-b",
                "target": "current",
                "kind": "prerequisite"
            }),
            json!({
                "id": "prereq-a->prereq-b",
                "source": "prereq-a",
                "target": "prereq-b",
                "kind": "prerequisite"
            })
        ]
    );
    assert_eq!(node_depth(&normalized, "current"), Some(0));
    assert_eq!(node_depth(&normalized, "prereq-b"), Some(1));
    assert_eq!(node_depth(&normalized, "prereq-a"), Some(2));
}

#[test]
fn normalize_graph_edges_adds_missing_prerequisite_nodes_from_task_list() {
    let payload = json!({
        "root_task_ids": ["current"],
        "nodes": [
            {
                "depth": 0,
                "is_root": true,
                "is_current_message": true,
                "task": {
                    "id": "current",
                    "title": "当前任务",
                    "source_session_id": "session-1",
                    "source_turn_id": "turn-1",
                    "source_user_message_id": "user-1",
                    "prerequisite_task_ids": ["prereq-a", "prereq-b"]
                }
            },
            {
                "depth": 1,
                "is_root": false,
                "is_current_message": false,
                "task": {
                    "id": "prereq-b",
                    "title": "前置 B",
                    "source_session_id": "session-1",
                    "source_turn_id": "turn-1",
                    "source_user_message_id": "user-1",
                    "prerequisite_task_ids": []
                }
            }
        ],
        "edges": [
            {
                "id": "prereq-b->current",
                "source": "prereq-b",
                "target": "current",
                "kind": "prerequisite"
            }
        ]
    });
    let supplemental_tasks = vec![json!({
        "id": "prereq-a",
        "title": "前置 A",
        "status": "succeeded",
        "source_session_id": "session-1",
        "source_turn_id": "turn-1",
        "source_user_message_id": "user-1",
        "prerequisite_task_ids": []
    })];

    let normalized =
        normalize_message_task_graph_payload_edges_with_tasks(payload, &supplemental_tasks);

    assert!(has_node(&normalized, "prereq-a"));
    assert_eq!(
        normalized_edges(&normalized),
        vec![
            json!({
                "id": "prereq-a->current",
                "source": "prereq-a",
                "target": "current",
                "kind": "prerequisite"
            }),
            json!({
                "id": "prereq-b->current",
                "source": "prereq-b",
                "target": "current",
                "kind": "prerequisite"
            })
        ]
    );
    assert_eq!(node_depth(&normalized, "current"), Some(0));
    assert_eq!(node_depth(&normalized, "prereq-a"), Some(1));
    assert_eq!(node_depth(&normalized, "prereq-b"), Some(1));
}

#[test]
fn normalize_graph_filters_subtask_nodes() {
    let payload = json!({
        "root_task_ids": ["root", "child"],
        "nodes": [
            {
                "depth": 0,
                "is_root": true,
                "is_current_message": true,
                "task": {
                    "id": "root",
                    "title": "主任务",
                    "prerequisite_task_ids": ["child"]
                }
            },
            {
                "depth": 0,
                "is_root": true,
                "is_current_message": true,
                "task": {
                    "id": "child",
                    "title": "子任务",
                    "parent_task_id": "root",
                    "prerequisite_task_ids": []
                }
            }
        ],
        "edges": [
            {
                "id": "child->root",
                "source": "child",
                "target": "root",
                "kind": "prerequisite"
            }
        ]
    });

    let normalized = normalize_message_task_graph_payload_edges(payload);

    assert!(has_node(&normalized, "root"));
    assert!(!has_node(&normalized, "child"));
    assert_eq!(
        normalized
            .get("root_task_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        vec![json!("root")]
    );
    assert!(normalized_edges(&normalized).is_empty());
}
