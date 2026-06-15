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
            creator_user_id: creator.map(|user| user.id.clone()),
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
        Ok(self
            .get_task_for_chatos_source(
                task_id,
                source_session_id,
                source_user_message_id,
                source_turn_id,
            )
            .await?
            .map(ChatosMessageTaskDetail::from))
    }
}
