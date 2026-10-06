// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_protocol::{
    GetMessageTaskGraphCommand, LocalMessageTaskGraph, LocalMessageTaskGraphEdge,
    LocalMessageTaskGraphNode, LocalTaskGraph, LocalTaskGraphListScope, LocalTaskRecord,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_MESSAGE_GRAPH_TASKS: usize = 200;

impl LocalAgentRuntime {
    pub(super) async fn message_task_graph(
        &self,
        command: GetMessageTaskGraphCommand,
    ) -> Result<LocalMessageTaskGraph, LocalAgentRuntimeError> {
        let current_graphs = self.current_turn_graphs(&command).await?;
        let mut graphs = current_graphs
            .iter()
            .cloned()
            .map(|graph| (graph.graph_id.clone(), graph))
            .collect::<HashMap<_, _>>();
        let mut root_task_ids = Vec::new();
        let mut ordered_ids = Vec::new();
        let mut tasks = HashMap::<String, LocalTaskRecord>::new();
        let mut depths = HashMap::<String, u32>::new();
        let mut queue = VecDeque::new();
        for graph in &current_graphs {
            for task in &graph.tasks {
                if task_conversation(task) != Some(command.source_conversation_id.as_str()) {
                    continue;
                }
                root_task_ids.push(task.task_id.clone());
                if tasks.insert(task.task_id.clone(), task.clone()).is_none() {
                    ordered_ids.push(task.task_id.clone());
                    queue.push_back(task.task_id.clone());
                }
                depths.insert(task.task_id.clone(), 0);
            }
        }
        if tasks.len() > MAX_MESSAGE_GRAPH_TASKS {
            return Err(LocalAgentRuntimeError::InvalidRequest(format!(
                "message Task Graph exceeds the {MAX_MESSAGE_GRAPH_TASKS} Task limit"
            )));
        }

        let mut prerequisite_edges = Vec::new();
        let mut prerequisite_edge_ids = HashSet::new();
        while let Some(task_id) = queue.pop_front() {
            let Some(task) = tasks.get(&task_id).cloned() else {
                continue;
            };
            let current_depth = depths.get(&task_id).copied().unwrap_or(0);
            let graph = self
                .graph_for_task(&command.owner_user_id, &task, &mut graphs)
                .await?;
            for prerequisite_id in direct_prerequisite_ids(&task, &graph) {
                let Some(prerequisite) = self
                    .store
                    .get_task_for_conversation(
                        &command.owner_user_id,
                        &command.source_conversation_id,
                        &prerequisite_id,
                    )
                    .await?
                else {
                    continue;
                };
                if task_conversation(&prerequisite) != Some(command.source_conversation_id.as_str())
                {
                    continue;
                }
                let edge_key = (prerequisite_id.clone(), task_id.clone());
                if prerequisite_edge_ids.insert(edge_key) {
                    prerequisite_edges.push(LocalMessageTaskGraphEdge {
                        source_task_id: prerequisite_id.clone(),
                        target_task_id: task_id.clone(),
                        kind: "prerequisite".to_string(),
                    });
                }
                let next_depth = current_depth.saturating_add(1);
                let needs_visit = match depths.get_mut(&prerequisite_id) {
                    Some(depth) if next_depth < *depth => {
                        *depth = next_depth;
                        true
                    }
                    Some(_) => false,
                    None => {
                        depths.insert(prerequisite_id.clone(), next_depth);
                        true
                    }
                };
                if tasks
                    .insert(prerequisite_id.clone(), prerequisite)
                    .is_none()
                {
                    ordered_ids.push(prerequisite_id.clone());
                    if tasks.len() > MAX_MESSAGE_GRAPH_TASKS {
                        return Err(LocalAgentRuntimeError::InvalidRequest(format!(
                            "message Task Graph exceeds the {MAX_MESSAGE_GRAPH_TASKS} Task limit"
                        )));
                    }
                }
                if needs_visit {
                    queue.push_back(prerequisite_id);
                }
            }
        }

        let task_ids = tasks.keys().cloned().collect::<HashSet<_>>();
        let mut edges = prerequisite_edges;
        let mut edge_ids = edges
            .iter()
            .map(|edge| {
                (
                    edge.source_task_id.clone(),
                    edge.target_task_id.clone(),
                    edge.kind.clone(),
                )
            })
            .collect::<HashSet<_>>();
        for graph in graphs.values() {
            append_context_edges(graph, &task_ids, &mut edge_ids, &mut edges);
        }
        let root_set = root_task_ids.iter().cloned().collect::<HashSet<_>>();
        let nodes = ordered_ids
            .into_iter()
            .filter_map(|task_id| {
                let task = tasks.remove(&task_id)?;
                Some(LocalMessageTaskGraphNode {
                    depth: depths.get(&task_id).copied().unwrap_or(0),
                    is_root: root_set.contains(&task_id),
                    is_current_message: root_set.contains(&task_id),
                    task,
                })
            })
            .collect();
        Ok(LocalMessageTaskGraph {
            root_task_ids,
            nodes,
            edges,
            source_conversation_id: command.source_conversation_id,
            source_turn_id: command.source_turn_id,
            source_user_message_id: command.source_user_message_id,
        })
    }

    async fn current_turn_graphs(
        &self,
        command: &GetMessageTaskGraphCommand,
    ) -> Result<Vec<LocalTaskGraph>, LocalAgentRuntimeError> {
        let mut graphs = Vec::new();
        let mut before_timestamp = None;
        let mut before_graph_id = None;
        loop {
            let page = self
                .store
                .list_task_graphs(
                    &command.owner_user_id,
                    LocalTaskGraphListScope::All,
                    Some("conversation_turn"),
                    Some(&command.source_turn_id),
                    before_timestamp,
                    before_graph_id.as_deref(),
                    100,
                )
                .await?;
            for summary in page.graphs {
                if let Some(graph) = self
                    .store
                    .get_task_graph(&command.owner_user_id, &summary.graph_id)
                    .await?
                {
                    graphs.push(graph);
                }
            }
            match (
                page.next_before_updated_at_unix_ms,
                page.next_before_graph_id,
            ) {
                (Some(timestamp), Some(graph_id)) => {
                    before_timestamp = Some(timestamp);
                    before_graph_id = Some(graph_id);
                }
                _ => break,
            }
        }
        Ok(graphs)
    }

    async fn graph_for_task(
        &self,
        owner_user_id: &str,
        task: &LocalTaskRecord,
        graphs: &mut HashMap<String, LocalTaskGraph>,
    ) -> Result<LocalTaskGraph, LocalAgentRuntimeError> {
        if let Some(graph) = graphs.get(&task.graph_id) {
            return Ok(graph.clone());
        }
        let graph = self
            .store
            .get_task_graph(owner_user_id, &task.graph_id)
            .await?
            .ok_or_else(|| {
                LocalAgentRuntimeError::InvalidRequest(format!(
                    "Task Graph not found: {}",
                    task.graph_id
                ))
            })?;
        graphs.insert(task.graph_id.clone(), graph.clone());
        Ok(graph)
    }
}

fn task_conversation(task: &LocalTaskRecord) -> Option<&str> {
    task.input
        .get("source_conversation_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn direct_prerequisite_ids(task: &LocalTaskRecord, graph: &LocalTaskGraph) -> Vec<String> {
    let mut seen = HashSet::new();
    graph
        .dependencies
        .iter()
        .filter(|edge| edge.task_id == task.task_id)
        .map(|edge| edge.prerequisite_task_id.as_str())
        .chain(
            task.input
                .get("prerequisite_task_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .map(str::trim)
        .filter(|value| !value.is_empty() && seen.insert((*value).to_string()))
        .map(str::to_string)
        .collect()
}

fn append_context_edges(
    graph: &LocalTaskGraph,
    included_task_ids: &HashSet<String>,
    edge_ids: &mut HashSet<(String, String, String)>,
    edges: &mut Vec<LocalMessageTaskGraphEdge>,
) {
    let by_client_ref = graph
        .tasks
        .iter()
        .filter_map(|task| {
            task_client_ref(task).map(|client_ref| (client_ref.to_string(), task.task_id.clone()))
        })
        .collect::<HashMap<_, _>>();
    for task in &graph.tasks {
        if !included_task_ids.contains(&task.task_id) {
            continue;
        }
        for context_ref in task_context_refs(task) {
            let Some(source_task_id) = by_client_ref.get(context_ref) else {
                continue;
            };
            if source_task_id == &task.task_id || !included_task_ids.contains(source_task_id) {
                continue;
            }
            let key = (
                source_task_id.clone(),
                task.task_id.clone(),
                "context".to_string(),
            );
            if edge_ids.insert(key) {
                edges.push(LocalMessageTaskGraphEdge {
                    source_task_id: source_task_id.clone(),
                    target_task_id: task.task_id.clone(),
                    kind: "context".to_string(),
                });
            }
        }
    }
}

fn task_client_ref(task: &LocalTaskRecord) -> Option<&str> {
    task.input
        .get("input_payload")
        .and_then(|payload| payload.get("execution_client_ref"))
        .and_then(Value::as_str)
        .or_else(|| task.input.get("client_ref").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn task_context_refs(task: &LocalTaskRecord) -> impl Iterator<Item = &str> {
    task.input
        .get("input_payload")
        .and_then(|payload| payload.get("dependency_context_refs"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
