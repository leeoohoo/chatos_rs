fn invocation_terminal_error_is_stale(error: &str) -> bool {
    matches!(
        error,
        "runtime session was not found or has expired"
            | "Runtime Tool Batch was not found"
            | "Runtime invocation was not found"
    )
}

#[cfg(test)]
mod invocation_terminal_tests {
    use super::{
        invocation_terminal_error_is_stale, is_recoverable_terminal_invocation_status,
        live_batch_watchdog_action, LiveBatchWatchdogAction,
    };
    use crate::runtime::RuntimeInvocationStatus;

    #[test]
    fn closed_session_and_removed_durable_records_are_consumed() {
        assert!(invocation_terminal_error_is_stale(
            "runtime session was not found or has expired"
        ));
        assert!(invocation_terminal_error_is_stale(
            "Runtime Tool Batch was not found"
        ));
        assert!(invocation_terminal_error_is_stale(
            "Runtime invocation was not found"
        ));
        assert!(!invocation_terminal_error_is_stale(
            "Runtime Tool Batch CAS conflict limit was exceeded"
        ));
    }

    #[test]
    fn restart_reconciliation_reduces_every_durable_terminal_invocation() {
        for status in [
            RuntimeInvocationStatus::Completed,
            RuntimeInvocationStatus::Failed,
            RuntimeInvocationStatus::Cancelled,
            RuntimeInvocationStatus::UnknownExecutionState,
        ] {
            assert!(is_recoverable_terminal_invocation_status(status));
        }
        for status in [
            RuntimeInvocationStatus::Queued,
            RuntimeInvocationStatus::Running,
            RuntimeInvocationStatus::WaitingForUser,
            RuntimeInvocationStatus::CancelRequested,
        ] {
            assert!(!is_recoverable_terminal_invocation_status(status));
        }
    }

    #[test]
    fn live_watchdog_republishes_queued_and_reduces_terminal_without_restart() {
        assert_eq!(
            live_batch_watchdog_action(RuntimeInvocationStatus::Queued),
            LiveBatchWatchdogAction::EnsureInvocationReady
        );
        for status in [
            RuntimeInvocationStatus::Completed,
            RuntimeInvocationStatus::Failed,
            RuntimeInvocationStatus::Cancelled,
            RuntimeInvocationStatus::UnknownExecutionState,
        ] {
            assert_eq!(
                live_batch_watchdog_action(status),
                LiveBatchWatchdogAction::ResumeTerminal
            );
        }
        for status in [
            RuntimeInvocationStatus::Running,
            RuntimeInvocationStatus::WaitingForUser,
            RuntimeInvocationStatus::CancelRequested,
        ] {
            assert_eq!(
                live_batch_watchdog_action(status),
                LiveBatchWatchdogAction::None
            );
        }
    }
}

async fn reconcile_pending_batches(
    state: &AppState,
    topology: &AsyncToolDispatchTopology,
    channel: &Channel,
) -> Result<(), String> {
    for batch in state.runtime_tool_batches.list_pending(1_000).await? {
        publish_batch_pending_event(state, topology, channel, &batch).await?;
    }
    Ok(())
}

async fn publish_command_to_queue(
    channel: &Channel,
    exchange: &str,
    queue_name: &str,
    command: &McpToolCallCommand,
) -> Result<(), String> {
    let payload = serde_json::to_vec(command).map_err(|error| error.to_string())?;
    publish_payload(channel, exchange, queue_name, payload.as_slice())
        .await
        .map_err(|error| error.to_string())
}

async fn publish_tool_call_result(
    channel: &Channel,
    exchange: &str,
    result_routing_key: &str,
    result: &McpToolCallResult,
) -> Result<(), String> {
    let payload = serde_json::to_vec(result).map_err(|error| error.to_string())?;
    let confirmation = channel
        .basic_publish(
            exchange,
            result_routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(result.event_id.clone().into())
                .with_correlation_id(result.batch_id.clone().into()),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable MCP tool call result for {result_routing_key}"
        )),
        Confirmation::Nack(_) => Err("RabbitMQ rejected MCP tool call result".to_string()),
        Confirmation::NotRequested => {
            Err("RabbitMQ publisher confirm is not enabled for MCP tool call results".to_string())
        }
    }
}

fn exhausted_tool_call_result(command: &McpToolCallCommand, error: &str) -> McpToolCallResult {
    McpToolCallResult {
        event_id: format!("mcp_batch_result_{}", command.batch_id),
        owner_service: command.owner_service.clone(),
        agent_run_id: command.agent_run_id.clone(),
        agent_key: command.agent_key.clone(),
        ordering_lane_key: command.ordering_lane_key.clone(),
        lane_seq: command.lane_seq,
        generation: command.generation,
        source_step_seq: command.source_step_seq,
        batch_id: command.batch_id.clone(),
        session_id: command.mcp_runtime_session_ref.clone(),
        items: command
            .calls
            .iter()
            .map(|call| McpToolCallResultItem {
                invocation_id: call.invocation_id.clone(),
                tool_call_id: call.tool_call_id.clone(),
                call_index: call.call_index,
                name: call.name.clone(),
                status: McpToolCallResultStatus::Failed,
                result: None,
                error_code: Some(MCP_ERROR_INTERNAL),
                error: Some(format!(
                    "MCP tool call command failed after {} attempts: {error}",
                    command.delivery_attempt.max(1)
                )),
            })
            .collect(),
    }
}

pub(super) async fn run_cancellation_consumer_loop(
    state: AppState,
    topology: AsyncToolDispatchTopology,
) {
    loop {
        match open_cancellation_consumer(&topology).await {
            Ok((connection, mut consumer)) => {
                let _connection = connection;
                state
                    .async_tool_dispatch
                    .set_cancellation_consumer_connected(true);
                if let Err(error) = state
                    .runtime_invocations
                    .reconcile_cancellation_waiters()
                    .await
                {
                    warn!(
                        error = error.as_str(),
                        "reconcile MCP invocation cancellation waiters failed"
                    );
                }
                while let Some(delivery) = consumer.next().await {
                    match delivery {
                        Ok(delivery) => {
                            match serde_json::from_slice::<InvocationCancellationEvent>(
                                delivery.data.as_slice(),
                            ) {
                                Ok(event) => {
                                    if let Err(error) = state
                                        .runtime_invocations
                                        .signal_cancellation(event.invocation_id.as_str())
                                    {
                                        warn!(
                                            invocation_id = event.invocation_id.as_str(),
                                            error = error.as_str(),
                                            "signal MCP invocation cancellation failed"
                                        );
                                    }
                                }
                                Err(error) => warn!(
                                    error = error.to_string().as_str(),
                                    "invalid MCP invocation cancellation event"
                                ),
                            }
                            if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
                                warn!(
                                    error = error.to_string().as_str(),
                                    "acknowledge MCP invocation cancellation event failed"
                                );
                            }
                        }
                        Err(error) => {
                            warn!(
                                error = error.to_string().as_str(),
                                "MCP invocation cancellation consumer stream failed"
                            );
                            break;
                        }
                    }
                }
                state
                    .async_tool_dispatch
                    .set_cancellation_consumer_connected(false);
            }
            Err(error) => {
                state
                    .async_tool_dispatch
                    .set_cancellation_consumer_connected(false);
                warn!(
                    error = error.as_str(),
                    "MCP invocation cancellation consumer failed to connect to rabbitmq"
                );
            }
        }
        tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
    }
}

pub(super) fn unavailable_rabbitmq_queue_stats() -> RabbitMqQueueRuntimeStats {
    RabbitMqQueueRuntimeStats {
        enabled: true,
        available: false,
        queues: Vec::new(),
        error: Some("rabbitmq_queue_inspection_unavailable".to_string()),
    }
}

pub(super) async fn open_rabbitmq_publisher(
    topology: &AsyncToolDispatchTopology,
) -> Result<RabbitMqPublisher, AsyncToolEnqueueError> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        AsyncToolEnqueueError::Unavailable(
            "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for RabbitMQ dispatch".to_string(),
        )
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    ensure_rabbitmq_topology(&channel, topology)
        .await
        .map_err(AsyncToolEnqueueError::Unavailable)?;
    let cancellation_exchange = topology.cancellation_exchange.clone().ok_or_else(|| {
        AsyncToolEnqueueError::Unavailable(
            "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for RabbitMQ dispatch"
                .to_string(),
        )
    })?;
    Ok(RabbitMqPublisher {
        _connection: connection,
        channel,
        cancellation_exchange,
    })
}

async fn publish_payload(
    channel: &Channel,
    exchange: &str,
    queue_name: &str,
    payload: &[u8],
) -> Result<(), AsyncToolEnqueueError> {
    let confirmation = channel
        .basic_publish(
            exchange,
            queue_name,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2),
        )
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    ensure_publish_confirmed(queue_name, confirmation)
}

pub(super) fn ensure_publish_confirmed(
    queue_name: &str,
    confirmation: Confirmation,
) -> Result<(), AsyncToolEnqueueError> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(AsyncToolEnqueueError::Unavailable(format!(
            "RabbitMQ returned unroutable MCP async tool event for {queue_name}"
        ))),
        Confirmation::Nack(_) => Err(AsyncToolEnqueueError::CapacityExhausted),
        Confirmation::NotRequested => Err(AsyncToolEnqueueError::Unavailable(
            "RabbitMQ publisher confirm was not enabled for MCP async tool event".to_string(),
        )),
    }
}

pub(super) async fn open_rabbitmq_consumer(
    topology: &AsyncToolDispatchTopology,
) -> Result<(Connection, Channel, lapin::Consumer), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for RabbitMQ dispatch".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    ensure_rabbitmq_topology(&channel, topology).await?;
    let prefetch_count = u16::try_from(topology.worker_concurrency).map_err(|_| {
        "MCP async tool worker concurrency exceeds RabbitMQ prefetch range".to_string()
    })?;
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let consumer = channel
        .basic_consume(
            topology.queue_name.as_deref().unwrap_or_default(),
            RABBITMQ_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, channel, consumer))
}

async fn ensure_rabbitmq_topology(
    channel: &Channel,
    topology: &AsyncToolDispatchTopology,
) -> Result<(), String> {
    let exchange = topology.rabbitmq_exchange.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE is required for RabbitMQ dispatch".to_string()
    })?;
    let queue_name = topology.queue_name.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE is required for RabbitMQ dispatch".to_string()
    })?;
    let retry_queue_name = topology.retry_queue_name.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE is required for RabbitMQ dispatch".to_string()
    })?;
    let dead_letter_queue_name = topology.dead_letter_queue_name.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE is required for RabbitMQ dispatch".to_string()
    })?;
    channel
        .exchange_declare(
            exchange,
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let cancellation_exchange = topology.cancellation_exchange.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for RabbitMQ dispatch"
            .to_string()
    })?;
    channel
        .exchange_declare(
            cancellation_exchange,
            ExchangeKind::Fanout,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let dispatch_arguments = dispatch_queue_arguments(topology);
    channel
        .queue_declare(
            queue_name,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            dispatch_arguments,
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            queue_name,
            exchange,
            queue_name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    for internal_queue in [
        invocation_queue_name(topology),
        terminal_queue_name(topology),
    ] {
        channel
            .queue_declare(
                internal_queue.as_str(),
                QueueDeclareOptions {
                    durable: true,
                    ..QueueDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        channel
            .queue_bind(
                internal_queue.as_str(),
                exchange,
                internal_queue.as_str(),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    let retry_delay_ms = u32::try_from(topology.retry_delay.as_millis())
        .map_err(|_| "MCP async retry delay is too large for RabbitMQ".to_string())?;
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert("x-message-ttl".into(), AMQPValue::LongUInt(retry_delay_ms));
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(exchange.into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(queue_name.into()),
    );
    channel
        .queue_declare(
            retry_queue_name,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            retry_arguments,
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            retry_queue_name,
            exchange,
            retry_queue_name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_declare(
            dead_letter_queue_name,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            dead_letter_queue_name,
            exchange,
            dead_letter_queue_name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn open_named_consumer(
    topology: &AsyncToolDispatchTopology,
    queue_name: &str,
    consumer_tag: &str,
) -> Result<(Connection, Channel, lapin::Consumer), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for RabbitMQ dispatch".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    ensure_rabbitmq_topology(&channel, topology).await?;
    let prefetch_count = u16::try_from(topology.worker_concurrency).map_err(|_| {
        "MCP async tool worker concurrency exceeds RabbitMQ prefetch range".to_string()
    })?;
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let consumer = channel
        .basic_consume(
            queue_name,
            consumer_tag,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, channel, consumer))
}

async fn open_cancellation_consumer(
    topology: &AsyncToolDispatchTopology,
) -> Result<(Connection, lapin::Consumer), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for cancellation events".to_string()
    })?;
    let cancellation_exchange = topology.cancellation_exchange.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for cancellation events"
            .to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .exchange_declare(
            cancellation_exchange,
            ExchangeKind::Fanout,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let queue = channel
        .queue_declare(
            "",
            QueueDeclareOptions {
                exclusive: true,
                auto_delete: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let queue_name = queue.name().as_str();
    channel
        .queue_bind(
            queue_name,
            cancellation_exchange,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let prefetch_count = u16::try_from(topology.worker_concurrency)
        .map_err(|_| "MCP cancellation consumer prefetch exceeds RabbitMQ range".to_string())?;
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let consumer = channel
        .basic_consume(
            queue_name,
            RABBITMQ_CANCELLATION_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, consumer))
}

pub(super) fn dispatch_queue_arguments(topology: &AsyncToolDispatchTopology) -> FieldTable {
    let mut dispatch_arguments = FieldTable::default();
    dispatch_arguments.insert(
        "x-max-length".into(),
        AMQPValue::LongUInt(topology.queue_max_length),
    );
    dispatch_arguments.insert(
        "x-max-length-bytes".into(),
        AMQPValue::LongLongInt(topology.queue_max_bytes as i64),
    );
    dispatch_arguments.insert(
        "x-overflow".into(),
        AMQPValue::LongString("reject-publish".into()),
    );
    dispatch_arguments
}
