use std::collections::{HashMap, HashSet, VecDeque};

use super::matching::{
    normalize_source_id, normalized_chatos_source, task_matches_source_user_message,
};
use super::*;

impl TaskService {
    pub async fn list_tasks_for_source_user_message(
        &self,
        source_user_message_id: &str,
        creator: Option<&CurrentUser>,
    ) -> Result<Vec<TaskRecord>, String> {
        let Some(source_user_message_id) = normalize_source_id(source_user_message_id) else {
            return Ok(Vec::new());
        };
        let filters = sanitize_task_list_filters(TaskListFilters {
            creator_user_id: creator
                .and_then(|user| user.effective_owner_user_id().map(ToOwned::to_owned)),
            ..TaskListFilters::default()
        });
        let tasks = self.store.list_tasks_filtered(&filters).await?;
        let tasks = tasks
            .into_iter()
            .filter(|task| task_matches_source_user_message(task, source_user_message_id.as_str()))
            .collect::<Vec<_>>();
        self.hydrate_tasks_prerequisites(tasks).await
    }

    pub async fn list_tasks_for_chatos_message(
        &self,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Vec<TaskRecord>, String> {
        self.list_tasks_for_chatos_source(source_session_id, Some(source_user_message_id), None)
            .await
    }

    pub async fn list_tasks_for_chatos_source(
        &self,
        source_session_id: &str,
        source_user_message_id: Option<&str>,
        source_turn_id: Option<&str>,
    ) -> Result<Vec<TaskRecord>, String> {
        let Some(source) =
            normalized_chatos_source(source_session_id, source_user_message_id, source_turn_id)
        else {
            return Ok(Vec::new());
        };
        let mut tasks = self
            .store
            .list_tasks_filtered(&TaskListFilters::default())
            .await?
            .into_iter()
            .filter(|task| source.matches_task(task))
            .collect::<Vec<_>>();
        tasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        self.hydrate_tasks_prerequisites(tasks).await
    }

    pub async fn list_message_task_summaries_for_chatos_message(
        &self,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Vec<ChatosMessageTaskSummary>, String> {
        self.list_message_task_summaries_for_chatos_source(
            source_session_id,
            Some(source_user_message_id),
            None,
        )
        .await
    }

    pub async fn list_message_task_summaries_for_chatos_source(
        &self,
        source_session_id: &str,
        source_user_message_id: Option<&str>,
        source_turn_id: Option<&str>,
    ) -> Result<Vec<ChatosMessageTaskSummary>, String> {
        Ok(self
            .list_tasks_for_chatos_source(source_session_id, source_user_message_id, source_turn_id)
            .await?
            .into_iter()
            .map(ChatosMessageTaskSummary::from)
            .collect())
    }

    pub async fn list_active_message_task_sources_for_chatos_session(
        &self,
        source_session_id: &str,
        source_user_message_ids: &[String],
        source_turn_ids: &[String],
    ) -> Result<Vec<ChatosActiveMessageTaskSource>, String> {
        let Some(source_session_id) = normalize_source_id(source_session_id) else {
            return Ok(Vec::new());
        };
        let source_user_message_ids = source_user_message_ids
            .iter()
            .filter_map(|id| normalize_source_id(id.as_str()))
            .collect::<Vec<_>>();
        let source_turn_ids = source_turn_ids
            .iter()
            .filter_map(|id| normalize_source_id(id.as_str()))
            .collect::<Vec<_>>();
        let source_user_message_id_set = source_user_message_ids
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let source_turn_id_set = source_turn_ids.iter().cloned().collect::<HashSet<_>>();
        let has_source_filters = !source_user_message_ids.is_empty() || !source_turn_ids.is_empty();
        let mut source_by_key = HashMap::<String, ChatosActiveMessageTaskSource>::new();

        for status in [TaskStatus::Ready, TaskStatus::Queued, TaskStatus::Running] {
            let tasks = self
                .store
                .list_tasks_filtered(&TaskListFilters {
                    status: Some(status),
                    source_session_id: Some(source_session_id.clone()),
                    source_user_message_ids: source_user_message_ids.clone(),
                    source_turn_ids: source_turn_ids.clone(),
                    ..TaskListFilters::default()
                })
                .await?;
            for task in tasks {
                if task.source_session_id.as_deref().map(str::trim)
                    != Some(source_session_id.as_str())
                {
                    continue;
                }
                let source_user_message_id = task
                    .source_user_message_id
                    .as_deref()
                    .and_then(normalize_source_id);
                let source_turn_id = task.source_turn_id.as_deref().and_then(normalize_source_id);
                if source_user_message_id.is_none() && source_turn_id.is_none() {
                    continue;
                }
                if has_source_filters {
                    let message_matches = source_user_message_id
                        .as_ref()
                        .is_some_and(|id| source_user_message_id_set.contains(id));
                    let turn_matches = source_turn_id
                        .as_ref()
                        .is_some_and(|id| source_turn_id_set.contains(id));
                    if !message_matches && !turn_matches {
                        continue;
                    }
                }
                let key = source_user_message_id
                    .clone()
                    .or_else(|| source_turn_id.as_ref().map(|id| format!("turn:{id}")))
                    .unwrap_or_default();
                let entry =
                    source_by_key
                        .entry(key)
                        .or_insert_with(|| ChatosActiveMessageTaskSource {
                            source_user_message_id: source_user_message_id.clone(),
                            source_turn_id: source_turn_id.clone(),
                            running_count: 0,
                            active_count: 0,
                        });
                entry.running_count += 1;
                entry.active_count += 1;
            }
        }

        let mut items = source_by_key.into_values().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.source_user_message_id
                .cmp(&right.source_user_message_id)
                .then_with(|| left.source_turn_id.cmp(&right.source_turn_id))
        });
        Ok(items)
    }

    pub async fn get_task_for_chatos_message(
        &self,
        task_id: &str,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Option<TaskRecord>, String> {
        self.get_task_for_chatos_source(
            task_id,
            source_session_id,
            Some(source_user_message_id),
            None,
        )
        .await
    }

    pub async fn get_task_for_chatos_source(
        &self,
        task_id: &str,
        source_session_id: &str,
        source_user_message_id: Option<&str>,
        source_turn_id: Option<&str>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(source) =
            normalized_chatos_source(source_session_id, source_user_message_id, source_turn_id)
        else {
            return Ok(None);
        };
        let Some(task) = self.get_task(task_id).await? else {
            return Ok(None);
        };
        if source.matches_task(&task) {
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    pub async fn get_message_task_detail_for_chatos_message(
        &self,
        task_id: &str,
        source_session_id: &str,
        source_user_message_id: &str,
    ) -> Result<Option<ChatosMessageTaskDetail>, String> {
        self.get_message_task_detail_for_chatos_source(
            task_id,
            source_session_id,
            Some(source_user_message_id),
            None,
        )
        .await
    }

    pub async fn get_message_task_detail_for_chatos_source(
        &self,
        task_id: &str,
        source_session_id: &str,
        source_user_message_id: Option<&str>,
        source_turn_id: Option<&str>,
    ) -> Result<Option<ChatosMessageTaskDetail>, String> {
        let Some(task) = self
            .get_task_for_chatos_source(
                task_id,
                source_session_id,
                source_user_message_id,
                source_turn_id,
            )
            .await?
        else {
            return Ok(None);
        };
        self.build_chatos_message_task_detail(task).await.map(Some)
    }

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
            let detail = self.build_chatos_message_task_detail(task).await?;
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

    async fn build_chatos_message_task_detail(
        &self,
        task: TaskRecord,
    ) -> Result<ChatosMessageTaskDetail, String> {
        let default_model_config_id = task.default_model_config_id.clone();
        let last_run_id = task.last_run_id.clone();
        let parent_task_id = task.parent_task_id.clone();
        let source_run_id = task.source_run_id.clone();
        let prerequisite_task_ids = task.prerequisite_task_ids.clone();

        let default_model_config = self
            .chatos_model_config_summary_for_id(default_model_config_id.as_deref())
            .await?;
        let last_run = self
            .chatos_run_summary_for_id(last_run_id.as_deref())
            .await?;
        let parent_task = self
            .chatos_task_summary_for_id(parent_task_id.as_deref())
            .await?;
        let source_run = self
            .chatos_run_summary_for_id(source_run_id.as_deref())
            .await?;
        let prerequisite_tasks = self
            .chatos_task_summaries_in_id_order(&prerequisite_task_ids)
            .await?;

        Ok(ChatosMessageTaskDetail::from_parts(
            task,
            default_model_config,
            last_run,
            parent_task,
            source_run,
            prerequisite_tasks,
        ))
    }

    async fn chatos_task_summaries_in_id_order(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let ids = ids
            .iter()
            .filter_map(|id| non_empty_chatos_id(id))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut summaries = self.store.get_task_summaries_by_ids(&ids).await?;
        let mut ordered = Vec::new();
        for id in ids {
            if let Some(index) = summaries
                .iter()
                .position(|summary| summary.id.as_str() == id.as_str())
            {
                ordered.push(summaries.remove(index));
            }
        }
        Ok(ordered)
    }

    async fn chatos_task_summary_for_id(
        &self,
        id: Option<&str>,
    ) -> Result<Option<TaskSummaryRecord>, String> {
        let Some(id) = id.and_then(non_empty_chatos_id) else {
            return Ok(None);
        };
        Ok(self
            .store
            .get_task_summaries_by_ids(&[id.to_string()])
            .await?
            .into_iter()
            .find(|task| task.id.as_str() == id))
    }

    async fn chatos_model_config_summary_for_id(
        &self,
        id: Option<&str>,
    ) -> Result<Option<ChatosMessageModelConfigSummary>, String> {
        let Some(id) = id.and_then(non_empty_chatos_id) else {
            return Ok(None);
        };
        Ok(self
            .store
            .get_model_config(id)
            .await?
            .map(ChatosMessageModelConfigSummary::from))
    }

    async fn chatos_run_summary_for_id(
        &self,
        id: Option<&str>,
    ) -> Result<Option<ChatosMessageTaskRunSummary>, String> {
        let Some(id) = id.and_then(non_empty_chatos_id) else {
            return Ok(None);
        };
        Ok(self
            .store
            .get_run(id)
            .await?
            .map(ChatosMessageTaskRunSummary::from))
    }
}

fn non_empty_chatos_id(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn task_belongs_to_source_session(task: &TaskRecord, source_session_id: &str) -> bool {
    task.source_session_id.as_deref().map(str::trim) == Some(source_session_id.trim())
}
