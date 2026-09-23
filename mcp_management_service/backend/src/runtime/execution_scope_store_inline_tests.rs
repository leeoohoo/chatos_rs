#[cfg(test)]
mod tests {
    use super::*;

    async fn attach(store: &RuntimeExecutionScopeStore, run_id: &str) {
        store
            .attach_session(
                "user-1",
                Some("project-1"),
                run_id,
                WorkspaceProviderKind::LocalConnector,
                format!("session-{run_id}").as_str(),
                Utc::now().timestamp() + 300,
            )
            .await
            .unwrap();
    }

    async fn acquire(
        store: &RuntimeExecutionScopeStore,
        invocation_id: &str,
    ) -> RuntimeExecutionTurnState {
        store
            .try_acquire_invocation_turn(
                "user-1",
                Some("project-1"),
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                invocation_id,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn session_renewal_preserves_generation() {
        let store = RuntimeExecutionScopeStore::memory();
        attach(&store, "run-1").await;
        let renewed = store
            .attach_session(
                "user-1",
                Some("project-1"),
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "session-run-1",
                Utc::now().timestamp() + 600,
            )
            .await
            .unwrap();
        assert_eq!(renewed, 1);
    }

    #[tokio::test]
    async fn one_run_executes_invocations_in_fifo_order() {
        let store = RuntimeExecutionScopeStore::memory();
        attach(&store, "run-1").await;
        store
            .enqueue_invocation_batch(
                "user-1",
                Some("project-1"),
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "batch-1",
                &[
                    ("invocation-1".to_string(), 0),
                    ("invocation-2".to_string(), 1),
                ],
            )
            .await
            .unwrap();
        assert_eq!(
            acquire(&store, "invocation-2").await,
            RuntimeExecutionTurnState::Waiting
        );
        assert_eq!(
            acquire(&store, "invocation-1").await,
            RuntimeExecutionTurnState::Acquired
        );
        store
            .release_invocation_turn(
                "user-1",
                Some("project-1"),
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "invocation-1",
            )
            .await
            .unwrap();
        assert_eq!(
            acquire(&store, "invocation-2").await,
            RuntimeExecutionTurnState::Acquired
        );
    }

    #[tokio::test]
    async fn invocation_only_release_advances_fifo_without_session_snapshot() {
        let store = RuntimeExecutionScopeStore::memory();
        attach(&store, "run-1").await;
        store
            .enqueue_invocation_batch(
                "user-1",
                Some("project-1"),
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "batch-1",
                &[
                    ("invocation-1".to_string(), 0),
                    ("invocation-2".to_string(), 1),
                ],
            )
            .await
            .unwrap();
        assert_eq!(
            acquire(&store, "invocation-1").await,
            RuntimeExecutionTurnState::Acquired
        );

        let released = store
            .release_invocation_turn_by_id_and_next("invocation-1")
            .await
            .unwrap();
        assert_eq!(released.next_invocation_id.as_deref(), Some("invocation-2"));
        assert_eq!(
            acquire(&store, "invocation-2").await,
            RuntimeExecutionTurnState::Acquired
        );
    }

    #[tokio::test]
    async fn exact_batch_replay_reuses_queue_entries() {
        let store = RuntimeExecutionScopeStore::memory();
        attach(&store, "run-1").await;
        let invocations = [
            ("invocation-1".to_string(), 0),
            ("invocation-2".to_string(), 1),
        ];
        let first = store
            .enqueue_invocation_batch(
                "user-1",
                Some("project-1"),
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "batch-1",
                &invocations,
            )
            .await
            .unwrap();
        let replay = store
            .enqueue_invocation_batch(
                "user-1",
                Some("project-1"),
                "run-1",
                WorkspaceProviderKind::LocalConnector,
                "batch-1",
                &invocations,
            )
            .await
            .unwrap();
        assert_eq!(first, replay);
        assert_eq!(store.queued_invocation_ids().await.len(), 2);
    }

    #[tokio::test]
    #[ignore = "requires MCP_MANAGEMENT_TEST_DATABASE_URL"]
    async fn postgresql_workers_claim_one_fifo_turn_at_a_time() {
        let database_url = std::env::var("MCP_MANAGEMENT_TEST_DATABASE_URL")
            .expect("MCP_MANAGEMENT_TEST_DATABASE_URL");
        let first = RuntimeExecutionScopeStore::connect(&database_url)
            .await
            .unwrap();
        let second = RuntimeExecutionScopeStore::connect(&database_url)
            .await
            .unwrap();
        let suffix = uuid::Uuid::new_v4().to_string();
        let run_id = format!("scope-contract-{suffix}");
        let invocation_one = format!("contract-invocation-1-{suffix}");
        let invocation_two = format!("contract-invocation-2-{suffix}");
        first
            .attach_session(
                "contract-user",
                Some("contract-project"),
                &run_id,
                WorkspaceProviderKind::LocalConnector,
                "contract-session",
                Utc::now().timestamp() + 300,
            )
            .await
            .unwrap();
        first
            .enqueue_invocation_batch(
                "contract-user",
                Some("contract-project"),
                &run_id,
                WorkspaceProviderKind::LocalConnector,
                "contract-batch",
                &[(invocation_one.clone(), 0), (invocation_two.clone(), 1)],
            )
            .await
            .unwrap();
        let (one, two) = tokio::join!(
            first.try_acquire_invocation_turn(
                "contract-user",
                Some("contract-project"),
                &run_id,
                WorkspaceProviderKind::LocalConnector,
                &invocation_one,
            ),
            second.try_acquire_invocation_turn(
                "contract-user",
                Some("contract-project"),
                &run_id,
                WorkspaceProviderKind::LocalConnector,
                &invocation_two,
            )
        );
        assert_eq!(one.unwrap(), RuntimeExecutionTurnState::Acquired);
        assert_eq!(two.unwrap(), RuntimeExecutionTurnState::Waiting);
        let released = first
            .release_invocation_turn_by_id_and_next(&invocation_one)
            .await
            .unwrap();
        assert_eq!(
            released.next_invocation_id.as_deref(),
            Some(invocation_two.as_str())
        );
        assert_eq!(
            second
                .try_acquire_invocation_turn(
                    "contract-user",
                    Some("contract-project"),
                    &run_id,
                    WorkspaceProviderKind::LocalConnector,
                    &invocation_two,
                )
                .await
                .unwrap(),
            RuntimeExecutionTurnState::Acquired
        );
    }
}
