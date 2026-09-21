#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        summary_consumer_enabled, summary_slot_is_active, wait_until_consumer_enabled,
        SummaryRequestedEnvelope,
    };
    use crate::models::EngineThread;
    use crate::pressure::{MemoryEnginePressurePolicy, PlatformPressureLevel};
    use crate::repositories::threads::SummaryDispatchOutbox;

    #[test]
    fn outbox_event_contains_only_scope_ids_and_version() {
        let event = SummaryDispatchOutbox {
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            thread_id: "thread-1".to_string(),
            summary_dispatch_version: 7,
            summary_dispatch_published_version: 6,
            summary_dispatch_consumed_version: 5,
        };

        let envelope = SummaryRequestedEnvelope::from_outbox(&event);

        assert_eq!(envelope.thread_id, "thread-1");
        assert_eq!(envelope.version, 7);
        assert_eq!(envelope.attempt, 0);
    }

    fn thread_with_summary_lock(expires_at: Option<&str>) -> EngineThread {
        EngineThread {
            id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            subject_id: "subject-1".to_string(),
            thread_type: "conversation".to_string(),
            external_thread_id: None,
            title: None,
            labels: None,
            metadata: None,
            status: "active".to_string(),
            summary_status: "running".to_string(),
            summary_job_run_id: Some("job-1".to_string()),
            summary_locked_at: Some("2026-01-01T00:00:00Z".to_string()),
            summary_lock_expires_at: expires_at.map(ToOwned::to_owned),
            pending_record_count: 1,
            pending_summary_tokens: 1_000,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            archived_at: None,
        }
    }

    #[test]
    fn queue_defers_only_for_a_live_summary_slot() {
        let now = "2026-01-01T00:05:00Z";
        assert!(summary_slot_is_active(
            &thread_with_summary_lock(Some("2026-01-01T00:10:00Z")),
            now,
        ));
        assert!(!summary_slot_is_active(
            &thread_with_summary_lock(Some("2026-01-01T00:04:59Z")),
            now,
        ));
        assert!(!summary_slot_is_active(
            &thread_with_summary_lock(None),
            now,
        ));
    }

    #[test]
    fn pressure_policy_enables_only_the_target_number_of_consumers() {
        let policy = MemoryEnginePressurePolicy {
            level: PlatformPressureLevel::Elevated,
            active_summary_concurrency: 2,
            reconcile_paused: false,
            refresh_interval: Duration::from_secs(5),
            queue_elevated_messages: 100,
            queue_critical_messages: 1_000,
        };

        assert!(summary_consumer_enabled(&policy, 0));
        assert!(summary_consumer_enabled(&policy, 1));
        assert!(!summary_consumer_enabled(&policy, 2));
        assert!(!summary_consumer_enabled(&policy, 3));
    }

    #[tokio::test]
    async fn paused_consumer_resumes_from_pressure_change_without_polling() {
        let elevated = MemoryEnginePressurePolicy {
            level: PlatformPressureLevel::Elevated,
            active_summary_concurrency: 1,
            reconcile_paused: false,
            refresh_interval: Duration::from_secs(5),
            queue_elevated_messages: 100,
            queue_critical_messages: 1_000,
        };
        let normal = MemoryEnginePressurePolicy {
            level: PlatformPressureLevel::Normal,
            active_summary_concurrency: 4,
            reconcile_paused: false,
            refresh_interval: Duration::from_secs(5),
            queue_elevated_messages: 100,
            queue_critical_messages: 1_000,
        };
        let (sender, mut receiver) = tokio::sync::watch::channel(elevated);
        let waiter =
            tokio::spawn(async move { wait_until_consumer_enabled(&mut receiver, 2).await });

        assert!(tokio::time::timeout(Duration::from_millis(20), async {
            while !waiter.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_err());
        sender.send_replace(normal);
        assert!(waiter.await.expect("consumer waiter task").is_ok());
    }
}
