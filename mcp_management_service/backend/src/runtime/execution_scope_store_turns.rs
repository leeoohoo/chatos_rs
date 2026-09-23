use super::*;

impl RuntimeExecutionScopeStore {
    pub async fn try_acquire_invocation_turn(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
        invocation_id: &str,
    ) -> Result<RuntimeExecutionTurnState, String> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let Some(scope) = scopes.get_mut(id.as_str()) else {
                    return Err("execution scope is missing while acquiring a turn".to_string());
                };
                if scope.status == "terminal" {
                    return Ok(RuntimeExecutionTurnState::Terminal);
                }
                if scope.running_invocation_id.as_deref() == Some(invocation_id) {
                    return Ok(RuntimeExecutionTurnState::Acquired);
                }
                if scope.running_invocation_id.is_some()
                    || scope
                        .invocation_queue
                        .first()
                        .is_none_or(|reference| reference.invocation_id != invocation_id)
                {
                    return Ok(RuntimeExecutionTurnState::Waiting);
                }
                scope.invocation_queue.remove(0);
                scope.running_invocation_id = Some(invocation_id.to_string());
                Ok(RuntimeExecutionTurnState::Acquired)
            }
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let (running, status) = lock_scope(&mut tx, id.as_str())
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        "execution scope is missing while acquiring a turn".to_string()
                    })?;
                if status == "terminal" {
                    return Ok(RuntimeExecutionTurnState::Terminal);
                }
                if running.as_deref() == Some(invocation_id) {
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(RuntimeExecutionTurnState::Acquired);
                }
                if running.is_some() {
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(RuntimeExecutionTurnState::Waiting);
                }
                let first = sqlx::query_scalar::<_, String>(
                    "SELECT invocation_id \
                     FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE scope_id=$1 AND status='queued' ORDER BY sequence LIMIT 1",
                )
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                if first.as_deref() != Some(invocation_id) {
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(RuntimeExecutionTurnState::Waiting);
                }
                sqlx::query(
                    "UPDATE mcp_management_runtime_execution_scope_queue_items \
                     SET status='running' \
                     WHERE scope_id=$1 AND invocation_id=$2 AND status='queued'",
                )
                .bind(&id)
                .bind(invocation_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                sqlx::query(
                    "UPDATE mcp_management_runtime_execution_scopes \
                     SET running_invocation_id=$2,updated_at=now() WHERE id=$1",
                )
                .bind(&id)
                .bind(invocation_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                tx.commit().await.map_err(|error| error.to_string())?;
                Ok(RuntimeExecutionTurnState::Acquired)
            }
        }
    }

    pub async fn release_invocation_turn(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
        invocation_id: &str,
    ) -> Result<(), String> {
        self.release_invocation_turn_and_next(
            owner_user_id,
            project_id,
            run_id,
            provider,
            invocation_id,
        )
        .await
        .map(|_| ())
    }

    pub async fn release_invocation_turn_and_next(
        &self,
        owner_user_id: &str,
        project_id: Option<&str>,
        run_id: &str,
        provider: WorkspaceProviderKind,
        invocation_id: &str,
    ) -> Result<ReleasedInvocationTurn, String> {
        let id = scope_id(owner_user_id, project_id, run_id, provider);
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                if let Some(scope) = scopes.write().await.get_mut(id.as_str()) {
                    if scope.running_invocation_id.as_deref() == Some(invocation_id) {
                        scope.running_invocation_id = None;
                    }
                    scope
                        .invocation_queue
                        .retain(|reference| reference.invocation_id != invocation_id);
                    return Ok(ReleasedInvocationTurn {
                        next_invocation_id: scope
                            .invocation_queue
                            .first()
                            .map(|reference| reference.invocation_id.clone()),
                    });
                }
                Ok(ReleasedInvocationTurn {
                    next_invocation_id: None,
                })
            }
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let Some((running, _)) = lock_scope(&mut tx, id.as_str())
                    .await
                    .map_err(|error| error.to_string())?
                else {
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(ReleasedInvocationTurn {
                        next_invocation_id: None,
                    });
                };
                sqlx::query(
                    "DELETE FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE scope_id=$1 AND invocation_id=$2",
                )
                .bind(&id)
                .bind(invocation_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                if running.as_deref() == Some(invocation_id) {
                    sqlx::query(
                        "UPDATE mcp_management_runtime_execution_scopes \
                         SET running_invocation_id=NULL,updated_at=now() WHERE id=$1",
                    )
                    .bind(&id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| error.to_string())?;
                }
                let next_invocation_id = sqlx::query_scalar::<_, String>(
                    "SELECT invocation_id \
                     FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE scope_id=$1 AND status='queued' ORDER BY sequence LIMIT 1",
                )
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                tx.commit().await.map_err(|error| error.to_string())?;
                Ok(ReleasedInvocationTurn { next_invocation_id })
            }
        }
    }

    pub async fn release_invocation_turn_by_id_and_next(
        &self,
        invocation_id: &str,
    ) -> Result<ReleasedInvocationTurn, String> {
        match self.backend.as_ref() {
            RuntimeExecutionScopeStoreBackend::Memory(scopes) => {
                let mut scopes = scopes.write().await;
                let Some(scope) = scopes.values_mut().find(|scope| {
                    scope.running_invocation_id.as_deref() == Some(invocation_id)
                        || scope
                            .invocation_queue
                            .iter()
                            .any(|reference| reference.invocation_id == invocation_id)
                }) else {
                    return Ok(ReleasedInvocationTurn {
                        next_invocation_id: None,
                    });
                };
                if scope.running_invocation_id.as_deref() == Some(invocation_id) {
                    scope.running_invocation_id = None;
                }
                scope
                    .invocation_queue
                    .retain(|reference| reference.invocation_id != invocation_id);
                Ok(ReleasedInvocationTurn {
                    next_invocation_id: scope
                        .invocation_queue
                        .first()
                        .map(|reference| reference.invocation_id.clone()),
                })
            }
            RuntimeExecutionScopeStoreBackend::Postgres(pool) => {
                let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
                let scope_id = sqlx::query_scalar::<_, String>(
                    "SELECT scope_id FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE invocation_id=$1",
                )
                .bind(invocation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                let Some(scope_id) = scope_id else {
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(ReleasedInvocationTurn {
                        next_invocation_id: None,
                    });
                };
                let Some((running, _)) = lock_scope(&mut tx, scope_id.as_str())
                    .await
                    .map_err(|error| error.to_string())?
                else {
                    tx.commit().await.map_err(|error| error.to_string())?;
                    return Ok(ReleasedInvocationTurn {
                        next_invocation_id: None,
                    });
                };
                sqlx::query(
                    "DELETE FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE scope_id=$1 AND invocation_id=$2",
                )
                .bind(scope_id.as_str())
                .bind(invocation_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                if running.as_deref() == Some(invocation_id) {
                    sqlx::query(
                        "UPDATE mcp_management_runtime_execution_scopes \
                         SET running_invocation_id=NULL,updated_at=now() WHERE id=$1",
                    )
                    .bind(scope_id.as_str())
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| error.to_string())?;
                }
                let next_invocation_id = sqlx::query_scalar::<_, String>(
                    "SELECT invocation_id \
                     FROM mcp_management_runtime_execution_scope_queue_items \
                     WHERE scope_id=$1 AND status='queued' ORDER BY sequence LIMIT 1",
                )
                .bind(scope_id.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                tx.commit().await.map_err(|error| error.to_string())?;
                Ok(ReleasedInvocationTurn { next_invocation_id })
            }
        }
    }
}
