#[cfg(test)]
mod tests {
    use super::{SubjectMemoryEnvelope, SCOPE_REQUESTED_EVENT, SOURCE_AVAILABLE_EVENT};
    use crate::repositories::{subject_memory_scopes, summaries};

    #[test]
    fn source_event_contains_summary_scope_ids_only() {
        let envelope =
            SubjectMemoryEnvelope::from_source(&summaries::SubjectMemorySourceDispatchOutbox {
                id: "summary-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "source-1".to_string(),
                thread_id: "thread-1".to_string(),
                summary_type: "thread_incremental".to_string(),
                subject_memory_source_dispatch_version: 3,
                subject_memory_source_dispatch_published_version: 2,
                subject_memory_source_dispatch_consumed_version: 1,
                subject_memory_source_dispatch_pending: true,
            });
        assert_eq!(envelope.event_type, SOURCE_AVAILABLE_EVENT);
        assert_eq!(envelope.summary_id.as_deref(), Some("summary-1"));
        assert_eq!(envelope.scope_key, None);
        assert_eq!(envelope.version, 3);
    }

    #[test]
    fn scope_event_contains_scope_identity_only() {
        let envelope = SubjectMemoryEnvelope::from_scope(
            &subject_memory_scopes::SubjectMemoryScopeDispatchOutbox {
                id: "scope-id".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "source-1".to_string(),
                scope_key: "scope-1".to_string(),
                subject_memory_dispatch_version: 4,
                subject_memory_dispatch_published_version: 3,
                subject_memory_dispatch_consumed_version: 2,
                subject_memory_dispatch_pending: true,
            },
        );
        assert_eq!(envelope.event_type, SCOPE_REQUESTED_EVENT);
        assert_eq!(envelope.scope_key.as_deref(), Some("scope-1"));
        assert_eq!(envelope.summary_id, None);
        assert_eq!(envelope.version, 4);
    }
}
