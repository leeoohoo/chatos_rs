// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet, VecDeque};

use super::super::{
    ChatosMessageTaskGraph, ChatosMessageTaskGraphEdge, ChatosMessageTaskGraphNode, TaskRecord,
};
use super::{non_empty_chatos_id, TaskService};

impl TaskService {
    pub async fn get_message_task_graph_for_chatos_message(
        &self,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<ChatosMessageTaskGraph, String> {
        self.get_message_task_graph_for_chatos_source(
            source_session_id,
            Some(source_user_message_id),
            None,
        )
        .await
    }

    pub async fn get_message_task_graph_for_chatos_source(
        &self,
        source_session_id: &str,
        source_user_message_id: Option<&str>,
        source_turn_id: Option<&str>,
    ) -> Result<ChatosMessageTaskGraph, String> {
        let root_tasks = self
            .list_tasks_for_chatos_source(source_session_id, source_user_message_id, source_turn_id)
            .await?;
        let root_task_ids = root_tasks
            .iter()
            .filter_map(|task| non_empty_chatos_id(task.id.as_str()))
            .map(str::to_string)
            .collect::<Vec<_>>();
        let root_task_id_set = root_task_ids.iter().cloned().collect::<HashSet<_>>();

        let mut ordered_ids = Vec::new();
        let mut depth_by_id = HashMap::<String, usize>::new();
        let mut tasks_by_id = HashMap::<String, TaskRecord>::new();
        let mut queue = VecDeque::<TaskRecord>::new();

        for task in root_tasks {
            let Some(task_id) = non_empty_chatos_id(task.id.as_str()).map(str::to_string) else {
                continue;
            };
            if !tasks_by_id.contains_key(task_id.as_str()) {
                ordered_ids.push(task_id.clone());
            }
            depth_by_id.entry(task_id.clone()).or_insert(0);
            tasks_by_id.insert(task_id, task.clone());
            queue.push_back(task);
        }

        while let Some(task) = queue.pop_front() {
            let Some(task_id) = non_empty_chatos_id(task.id.as_str()) else {
                continue;
            };
            let current_depth = depth_by_id.get(task_id).copied().unwrap_or(0);
            for prerequisite_task_id in task.prerequisite_task_ids.iter() {
                let Some(prerequisite_task_id) =
                    non_empty_chatos_id(prerequisite_task_id.as_str()).map(str::to_string)
                else {
                    continue;
                };
                depth_by_id
                    .entry(prerequisite_task_id.clone())
                    .and_modify(|depth| {
                        if current_depth + 1 < *depth {
                            *depth = current_depth + 1;
                        }
                    })
                    .or_insert(current_depth + 1);
                if tasks_by_id.contains_key(prerequisite_task_id.as_str()) {
                    continue;
                }
                let Some(prerequisite_task) = self.get_task(prerequisite_task_id.as_str()).await?
                else {
                    continue;
                };
                if !task_is_top_level(&prerequisite_task) {
                    continue;
                }
                if !task_belongs_to_source_session(&prerequisite_task, source_session_id) {
                    continue;
                }
                ordered_ids.push(prerequisite_task_id.clone());
                tasks_by_id.insert(prerequisite_task_id, prerequisite_task.clone());
                queue.push_back(prerequisite_task);
            }
        }

        let mut edges = Vec::new();
        let mut edge_ids = HashSet::new();
        let graph_task_ids = tasks_by_id.keys().cloned().collect::<HashSet<_>>();
        for task_id in &ordered_ids {
            let Some(task) = tasks_by_id.get(task_id.as_str()) else {
                continue;
            };
            for prerequisite_task_id in task.prerequisite_task_ids.iter() {
                let Some(prerequisite_task_id) =
                    non_empty_chatos_id(prerequisite_task_id.as_str()).map(str::to_string)
                else {
                    continue;
                };
                if !graph_task_ids.contains(prerequisite_task_id.as_str()) {
                    continue;
                }
                let edge_id = format!("{prerequisite_task_id}->{task_id}");
                if !edge_ids.insert(edge_id.clone()) {
                    continue;
                }
                edges.push(ChatosMessageTaskGraphEdge {
                    id: edge_id,
                    source: prerequisite_task_id,
                    target: task_id.clone(),
                    kind: "prerequisite".to_string(),
                });
            }
        }

        let mut nodes = Vec::with_capacity(ordered_ids.len());
        for task_id in &ordered_ids {
            let Some(task) = tasks_by_id.get(task_id.as_str()).cloned() else {
                continue;
            };
            let mut detail = self.build_chatos_message_task_detail(task).await?;
            detail
                .prerequisite_task_ids
                .retain(|prerequisite_task_id| graph_task_ids.contains(prerequisite_task_id));
            detail
                .prerequisite_tasks
                .retain(|prerequisite_task| graph_task_ids.contains(prerequisite_task.id.as_str()));
            nodes.push(ChatosMessageTaskGraphNode {
                task: detail,
                depth: depth_by_id.get(task_id.as_str()).copied().unwrap_or(0),
                is_root: root_task_id_set.contains(task_id.as_str()),
                is_current_message: root_task_id_set.contains(task_id.as_str()),
            });
        }

        Ok(ChatosMessageTaskGraph {
            root_task_ids,
            nodes,
            edges,
            source_session_id: source_session_id.to_string(),
            source_turn_id: source_turn_id.map(str::to_string),
            source_user_message_id: source_user_message_id.map(str::to_string),
        })
    }
}

fn task_belongs_to_source_session(task: &TaskRecord, source_session_id: &str) -> bool {
    task.source_session_id.as_deref().map(str::trim) == Some(source_session_id.trim())
}

fn task_is_top_level(task: &TaskRecord) -> bool {
    task.parent_task_id
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
}
