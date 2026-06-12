use std::collections::HashSet;

use crate::auth::CurrentUser;
use crate::models::{now_rfc3339, TaskDependencyGraph, TaskRecord, TaskStatus, TaskSummaryRecord};

use super::batch_ops::normalize_prerequisite_task_ids;
use super::TaskService;

impl TaskService {
    pub async fn list_task_prerequisites(
        &self,
        id: &str,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        if self.store.get_task(id).await?.is_none() {
            return Err(format!("任务不存在: {id}"));
        }
        let ids = self.direct_prerequisite_ids(id).await?;
        self.store.get_task_summaries_by_ids(&ids).await
    }

    pub async fn set_task_prerequisites(
        &self,
        id: &str,
        prerequisite_task_ids: Vec<String>,
        current_user: Option<&CurrentUser>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let prerequisite_task_ids = normalize_prerequisite_task_ids(prerequisite_task_ids);
        self.validate_task_prerequisites(id, &prerequisite_task_ids, current_user)
            .await?;
        self.store
            .set_task_prerequisites(id, prerequisite_task_ids.clone())
            .await?;
        task.prerequisite_task_ids = prerequisite_task_ids;
        task.updated_at = now_rfc3339();
        let saved = self.store.save_task(task).await?;
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn get_task_dependency_graph(
        &self,
        id: &str,
    ) -> Result<Option<TaskDependencyGraph>, String> {
        if self.store.get_task(id).await?.is_none() {
            return Ok(None);
        }
        let direct_ids = self.direct_prerequisite_ids(id).await?;
        let transitive_ids = self.resolve_prerequisite_order(id).await?;
        let direct = self.store.get_task_summaries_by_ids(&direct_ids).await?;
        let transitive = self
            .store
            .get_task_summaries_by_ids(&transitive_ids)
            .await?;
        let blocked_by = transitive
            .iter()
            .filter(|task| task.status != TaskStatus::Succeeded)
            .cloned()
            .collect::<Vec<_>>();
        Ok(Some(TaskDependencyGraph {
            task_id: id.to_string(),
            prerequisites: direct,
            transitive_prerequisites: transitive,
            ready: blocked_by.is_empty(),
            blocked_by,
        }))
    }

    pub(super) async fn hydrate_task_prerequisites(
        &self,
        mut task: TaskRecord,
    ) -> Result<TaskRecord, String> {
        task.prerequisite_task_ids = self.direct_prerequisite_ids(&task.id).await?;
        Ok(task)
    }

    pub(super) async fn hydrate_tasks_prerequisites(
        &self,
        tasks: Vec<TaskRecord>,
    ) -> Result<Vec<TaskRecord>, String> {
        let mut out = Vec::with_capacity(tasks.len());
        for task in tasks {
            out.push(self.hydrate_task_prerequisites(task).await?);
        }
        Ok(out)
    }

    pub(super) async fn direct_prerequisite_ids(
        &self,
        task_id: &str,
    ) -> Result<Vec<String>, String> {
        Ok(self
            .store
            .list_task_prerequisites(task_id)
            .await?
            .into_iter()
            .map(|item| item.prerequisite_task_id)
            .collect())
    }

    pub(super) async fn validate_task_prerequisites(
        &self,
        task_id: &str,
        prerequisite_task_ids: &[String],
        current_user: Option<&CurrentUser>,
    ) -> Result<(), String> {
        if prerequisite_task_ids.len() > 50 {
            return Err("前置任务数量不能超过 50 个".to_string());
        }
        for prerequisite_task_id in prerequisite_task_ids {
            if prerequisite_task_id == task_id {
                return Err("任务不能依赖自身".to_string());
            }
            let prerequisite = self
                .store
                .get_task(prerequisite_task_id)
                .await?
                .ok_or_else(|| format!("前置任务不存在: {prerequisite_task_id}"))?;
            if let Some(user) = current_user {
                if !user.is_admin()
                    && prerequisite.creator_user_id.as_deref() != Some(user.id.as_str())
                {
                    return Err(format!("无权引用前置任务: {prerequisite_task_id}"));
                }
            }
        }

        let mut stack = prerequisite_task_ids.to_vec();
        let mut visited = HashSet::new();
        let mut visited_count = 0usize;
        while let Some(current) = stack.pop() {
            if current == task_id {
                return Err(format!(
                    "前置任务不能形成循环依赖，任务 {task_id} 会依赖自身"
                ));
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            visited_count += 1;
            if visited_count > 200 {
                return Err("前置任务依赖链过深或过大，请拆分后再保存".to_string());
            }
            for edge in self.store.list_task_prerequisites(&current).await? {
                stack.push(edge.prerequisite_task_id);
            }
        }
        Ok(())
    }

    pub(super) async fn resolve_prerequisite_order(
        &self,
        task_id: &str,
    ) -> Result<Vec<String>, String> {
        let mut stack = vec![(task_id.to_string(), false)];
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut order = Vec::new();

        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                visiting.remove(&current);
                if visited.insert(current.clone()) && current != task_id {
                    order.push(current);
                }
                continue;
            }
            if visited.contains(&current) {
                continue;
            }
            if !visiting.insert(current.clone()) {
                return Err(format!("前置任务不能形成循环依赖: {current}"));
            }
            if visiting.len() > 200 {
                return Err("前置任务依赖链过深或过大，请拆分后再执行".to_string());
            }
            stack.push((current.clone(), true));
            let mut prerequisites = self.direct_prerequisite_ids(&current).await?;
            prerequisites.reverse();
            for prerequisite_task_id in prerequisites {
                if prerequisite_task_id == task_id {
                    return Err(format!(
                        "前置任务不能形成循环依赖，任务 {task_id} 会依赖自身"
                    ));
                }
                stack.push((prerequisite_task_id, false));
            }
        }
        Ok(order)
    }
}
