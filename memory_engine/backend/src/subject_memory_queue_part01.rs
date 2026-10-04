async fn open_publisher(config: &AppConfig) -> Result<(lapin::Connection, Channel), String> {
    rabbitmq_queue::open_publisher(config.rabbitmq_url.as_str(), &queue_topology(config)).await
}

async fn open_consumer(
    config: &AppConfig,
    consumer_index: usize,
) -> Result<(lapin::Connection, Channel, lapin::Consumer), String> {
    let consumer_tag = format!("memory-engine-subject-memory-{consumer_index}");
    rabbitmq_queue::open_consumer(
        config.rabbitmq_url.as_str(),
        &queue_topology(config),
        consumer_tag.as_str(),
    )
    .await
}

async fn run_outbox_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(state.config.subject_memory_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut recovery_offset = 0_u64;
    loop {
        interval.tick().await;
        match reconcile_outbox(&state, recovery_offset).await {
            Ok((count, next_offset)) => {
                recovery_offset = next_offset;
                if count > 0 {
                    info!(
                        published_count = count,
                        recovery_offset,
                        "Memory Engine reconciled pending subject memory Outbox events"
                    );
                }
            }
            Err(err) => warn!(
                error = err.as_str(),
                "Memory Engine failed to reconcile subject memory Outbox events"
            ),
        }
    }
}

async fn reconcile_outbox(state: &AppState, recovery_offset: u64) -> Result<(usize, u64), String> {
    let (_connection, channel) = open_publisher(&state.config).await?;
    let mut published = 0usize;
    let source_events = summaries::list_pending_subject_memory_source_dispatches(
        &state.pool,
        state.config.subject_memory_outbox_batch_size,
    )
    .await?;
    for event in source_events {
        publish_source_outbox(&state.pool, &state.config, &channel, &event).await?;
        published += 1;
    }
    let scope_events = subject_memory_scopes::list_pending_subject_memory_dispatches(
        &state.pool,
        state.config.subject_memory_outbox_batch_size,
    )
    .await?;
    for event in scope_events {
        publish_scope_outbox(&state.pool, &state.config, &channel, &event).await?;
        published += 1;
    }

    let policy = control_plane::get_effective_job_policy(&state.pool, "subject_memory").await?;
    if !policy.enabled {
        return Ok((published, recovery_offset));
    }
    let scopes = subject_memory_scopes::list_active_subject_memory_scopes_page(
        &state.pool,
        None,
        None,
        state.config.subject_memory_outbox_batch_size,
        recovery_offset,
    )
    .await?;
    let scanned_count = scopes.len() as u64;
    for scope in scopes {
        if !subject_memory::scope_has_pending_work(&state.pool, &scope).await? {
            continue;
        }
        let Some(event) = subject_memory_scopes::rearm_subject_memory_dispatch(
            &state.pool,
            scope.tenant_id.as_str(),
            scope.source_id.as_str(),
            scope.scope_key.as_str(),
        )
        .await?
        else {
            continue;
        };
        if event.subject_memory_dispatch_pending {
            publish_scope_outbox(&state.pool, &state.config, &channel, &event).await?;
            published += 1;
        }
    }
    let batch_size = state.config.subject_memory_outbox_batch_size.max(1) as u64;
    let next_offset = if scanned_count < batch_size {
        0
    } else {
        recovery_offset.saturating_add(scanned_count)
    };
    Ok((published, next_offset))
}

#[cfg(test)]
include!("subject_memory_queue_inline_tests.rs");
