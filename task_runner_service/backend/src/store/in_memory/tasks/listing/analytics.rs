use super::*;

impl InMemoryStore {
    pub(in crate::store) fn task_stats(&self) -> TaskStatsResponse {
        let data = self.inner.read();
        let mut stats = empty_task_stats();

        for task in data.tasks.values() {
            stats.total += 1;
            if !matches!(task.schedule.mode, TaskScheduleMode::Manual) {
                stats.scheduled += 1;
            }
            if task.parent_task_id.is_some() {
                stats.follow_up += 1;
            }
            match task.status {
                TaskStatus::Draft => stats.draft += 1,
                TaskStatus::Ready => stats.ready += 1,
                TaskStatus::Running => stats.running += 1,
                TaskStatus::Succeeded => stats.succeeded += 1,
                TaskStatus::Failed => stats.failed += 1,
                TaskStatus::Blocked => stats.blocked += 1,
                TaskStatus::Cancelled => stats.cancelled += 1,
                TaskStatus::Archived => stats.archived += 1,
            }
        }

        stats
    }

    pub(in crate::store) fn list_due_scheduled_tasks(&self, now: DateTime<Utc>) -> Vec<TaskRecord> {
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .filter(|task| task_due_for_scheduler(task, &now))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            task_due_at(left)
                .cmp(&task_due_at(right))
                .then(left.id.cmp(&right.id))
        });
        items
    }
}
