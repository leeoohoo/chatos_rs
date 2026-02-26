mod mapper;
mod normalizer;
mod review_hub;
mod store;
mod types;

pub use review_hub::{
    create_task_review, submit_task_review_decision, wait_for_task_review_decision,
};
pub use store::{
    complete_task_by_id, create_tasks_for_turn, delete_task_by_id, list_tasks_for_context,
    update_task_by_id,
};
#[allow(unused_imports)]
pub use types::{
    TaskCreateReviewPayload, TaskDraft, TaskRecord, TaskReviewAction, TaskReviewDecision,
    TaskUpdatePatch, REVIEW_NOT_FOUND_ERR, REVIEW_TIMEOUT_ERR, REVIEW_TIMEOUT_MS_DEFAULT,
    TASK_NOT_FOUND_ERR,
};

#[cfg(test)]
mod tests {
    use super::normalizer::normalize_task_draft;
    use super::{
        create_task_review, submit_task_review_decision, wait_for_task_review_decision, TaskDraft,
        TaskReviewAction, TaskUpdatePatch,
    };

    #[test]
    fn normalize_task_draft_applies_defaults() {
        let draft = TaskDraft {
            title: "  Build review panel  ".to_string(),
            details: "  Some details  ".to_string(),
            priority: "unknown".to_string(),
            status: "invalid".to_string(),
            tags: vec![" ui ".to_string(), "ui".to_string(), "".to_string()],
            due_at: Some("  ".to_string()),
        };

        let normalized = normalize_task_draft(draft).expect("normalize should succeed");
        assert_eq!(normalized.title, "Build review panel");
        assert_eq!(normalized.details, "Some details");
        assert_eq!(normalized.priority, "medium");
        assert_eq!(normalized.status, "todo");
        assert_eq!(normalized.tags, vec!["ui"]);
        assert_eq!(normalized.due_at, None);
    }

    #[test]
    fn normalize_update_patch_applies_defaults() {
        let patch = TaskUpdatePatch {
            title: Some("  Refine workbar  ".to_string()),
            details: Some("  trim me  ".to_string()),
            priority: Some("unknown".to_string()),
            status: Some("invalid".to_string()),
            tags: Some(vec![" ui ".to_string(), "ui".to_string(), "".to_string()]),
            due_at: Some(Some("  ".to_string())),
        };

        let normalized = patch.normalized().expect("patch normalize should succeed");
        assert_eq!(normalized.title.as_deref(), Some("Refine workbar"));
        assert_eq!(normalized.details.as_deref(), Some("trim me"));
        assert_eq!(normalized.priority.as_deref(), Some("medium"));
        assert_eq!(normalized.status.as_deref(), Some("todo"));
        assert_eq!(normalized.tags, Some(vec!["ui".to_string()]));
        assert_eq!(normalized.due_at, Some(None));
    }

    #[tokio::test]
    async fn review_confirm_flow_returns_updated_tasks() {
        let draft = TaskDraft {
            title: "Initial task".to_string(),
            details: "detail".to_string(),
            priority: "medium".to_string(),
            status: "todo".to_string(),
            tags: vec!["one".to_string()],
            due_at: None,
        };

        let (payload, receiver) =
            create_task_review("session_test", "turn_test", vec![draft], 30_000)
                .await
                .expect("create review should succeed");

        let updated_tasks = vec![TaskDraft {
            title: "Updated task".to_string(),
            details: "updated".to_string(),
            priority: "high".to_string(),
            status: "doing".to_string(),
            tags: vec!["backend".to_string()],
            due_at: Some("2026-03-01T10:00:00Z".to_string()),
        }];

        submit_task_review_decision(
            payload.review_id.as_str(),
            TaskReviewAction::Confirm,
            Some(updated_tasks.clone()),
            None,
        )
        .await
        .expect("submit decision should succeed");

        let decision = wait_for_task_review_decision(payload.review_id.as_str(), receiver, 5_000)
            .await
            .expect("wait decision should succeed");

        assert_eq!(decision.action, TaskReviewAction::Confirm);
        assert_eq!(decision.tasks.len(), 1);
        assert_eq!(decision.tasks[0].title, "Updated task");
        assert_eq!(decision.tasks[0].priority, "high");
        assert_eq!(decision.tasks[0].status, "doing");
    }

    #[tokio::test]
    async fn review_cancel_flow_returns_cancel_action() {
        let draft = TaskDraft {
            title: "Cancel me".to_string(),
            details: String::new(),
            priority: "medium".to_string(),
            status: "todo".to_string(),
            tags: Vec::new(),
            due_at: None,
        };

        let (payload, receiver) =
            create_task_review("session_test", "turn_cancel", vec![draft], 30_000)
                .await
                .expect("create review should succeed");

        submit_task_review_decision(
            payload.review_id.as_str(),
            TaskReviewAction::Cancel,
            None,
            Some("user_cancelled".to_string()),
        )
        .await
        .expect("cancel decision should succeed");

        let decision = wait_for_task_review_decision(payload.review_id.as_str(), receiver, 5_000)
            .await
            .expect("wait decision should succeed");

        assert_eq!(decision.action, TaskReviewAction::Cancel);
        assert!(decision.tasks.is_empty());
        assert_eq!(decision.reason.as_deref(), Some("user_cancelled"));
    }
}
