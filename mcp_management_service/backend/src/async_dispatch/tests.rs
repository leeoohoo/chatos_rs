use futures_util::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions},
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable},
};

use super::*;

const TEST_RABBITMQ_URL_ENV: &str = "CHATOS_MCP_MANAGEMENT_TEST_RABBITMQ_URL";

fn envelope() -> QueuedAsyncToolCallEnvelope {
    QueuedAsyncToolCallEnvelope {
        invocation_id: "invocation-1".to_string(),
        session_id: "session-1".to_string(),
        resource_id: "resource-1".to_string(),
        exposed_tool_name: "tool-1".to_string(),
        arguments: serde_json::json!({}),
        mutation_may_have_started: false,
        delivery_attempt: INITIAL_DELIVERY_ATTEMPT,
    }
}

#[test]
fn delivery_retry_is_bounded_and_monotonic() {
    let first = envelope();
    let second = first.next_retry(3).expect("second attempt");
    let third = second.next_retry(3).expect("third attempt");

    assert_eq!(second.delivery_attempt, 2);
    assert_eq!(third.delivery_attempt, 3);
    assert!(third.next_retry(3).is_none());
}

#[test]
fn legacy_envelope_without_attempt_starts_at_one() {
    let value = serde_json::json!({
        "invocation_id": "invocation-1",
        "session_id": "session-1",
        "resource_id": "resource-1",
        "exposed_tool_name": "tool-1",
        "arguments": {},
        "mutation_may_have_started": false
    });
    let envelope = serde_json::from_value::<QueuedAsyncToolCallEnvelope>(value).unwrap();

    assert_eq!(envelope.delivery_attempt, INITIAL_DELIVERY_ATTEMPT);
}

#[test]
fn dispatch_queue_arguments_apply_hard_backpressure_limits() {
    let topology = crate::config::AppConfig::test().async_tool_dispatch_topology;
    let arguments = dispatch_queue_arguments(&topology);

    assert_eq!(
        arguments.inner().get("x-max-length"),
        Some(&AMQPValue::LongUInt(topology.queue_max_length))
    );
    assert_eq!(
        arguments.inner().get("x-max-length-bytes"),
        Some(&AMQPValue::LongLongInt(topology.queue_max_bytes as i64))
    );
    assert_eq!(
        arguments.inner().get("x-overflow"),
        Some(&AMQPValue::LongString("reject-publish".into()))
    );
}

#[test]
fn publisher_nack_is_reported_as_queue_backpressure() {
    let error = ensure_publish_confirmed("mcp.async", Confirmation::Nack(None))
        .expect_err("publisher nack must fail");
    assert_eq!(error, AsyncToolEnqueueError::CapacityExhausted);
}

#[test]
fn enqueue_runtime_stats_distinguish_capacity_from_infrastructure_failure() {
    let dispatch =
        AsyncToolDispatch::new(crate::config::AppConfig::test().async_tool_dispatch_topology);
    dispatch.record_enqueue_result(&Ok(()));
    dispatch.record_enqueue_result(&Err(AsyncToolEnqueueError::CapacityExhausted));
    dispatch.record_enqueue_result(&Err(AsyncToolEnqueueError::Unavailable(
        "connection failed".to_string(),
    )));

    let stats = dispatch.runtime_stats();
    assert_eq!(stats.enqueue_accepted_total, 1);
    assert_eq!(stats.enqueue_capacity_rejected_total, 1);
    assert_eq!(stats.enqueue_unavailable_total, 1);
}

#[tokio::test]
#[ignore = "requires CHATOS_MCP_MANAGEMENT_TEST_RABBITMQ_URL"]
async fn rabbitmq_recovers_publisher_and_routes_retry_and_exhaustion_to_dlq() {
    let rabbitmq_url = std::env::var(TEST_RABBITMQ_URL_ENV).expect(TEST_RABBITMQ_URL_ENV);
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let mut topology = crate::config::AppConfig::test().async_tool_dispatch_topology;
    topology.mode = AsyncToolDispatchMode::RabbitMq;
    topology.worker_concurrency = 1;
    topology.rabbitmq_reconnect_delay = std::time::Duration::from_millis(50);
    topology.retry_delay = std::time::Duration::from_millis(150);
    topology.max_delivery_attempts = 2;
    topology.rabbitmq_url = Some(rabbitmq_url);
    topology.rabbitmq_exchange = Some(format!("mcp.test.{suffix}"));
    topology.cancellation_exchange = Some(format!("mcp.test.cancel.{suffix}"));
    topology.queue_name = Some(format!("mcp.test.dispatch.{suffix}"));
    topology.retry_queue_name = Some(format!("mcp.test.retry.{suffix}"));
    topology.dead_letter_queue_name = Some(format!("mcp.test.dlq.{suffix}"));

    let dispatch = AsyncToolDispatch::new(topology.clone());
    let original_publisher = dispatch
        .rabbitmq_publisher()
        .await
        .expect("open test publisher");
    let (_consumer_connection, consumer_channel, mut consumer) = open_rabbitmq_consumer(&topology)
        .await
        .expect("open test consumer");

    let mut first = envelope();
    first.invocation_id = format!("publisher-before-close-{suffix}");
    dispatch
        .enqueue(first.clone())
        .await
        .expect("publish first");
    let first_delivery = tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
        .await
        .expect("first delivery timeout")
        .expect("first consumer ended")
        .expect("first delivery");
    assert_eq!(
        serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&first_delivery.data)
            .expect("decode first delivery")
            .invocation_id,
        first.invocation_id
    );
    first_delivery
        .ack(BasicAckOptions::default())
        .await
        .expect("ack first delivery");

    original_publisher
        ._connection
        .close(200, "test publisher disconnect")
        .await
        .expect("close test publisher connection");
    let mut after_disconnect = envelope();
    after_disconnect.invocation_id = format!("publisher-after-close-{suffix}");
    assert!(matches!(
        dispatch.enqueue(after_disconnect.clone()).await,
        Err(AsyncToolEnqueueError::Unavailable(_))
    ));
    assert!(!dispatch.runtime_stats().publisher_connected);

    dispatch
        .enqueue(after_disconnect.clone())
        .await
        .expect("publisher reconnects on next enqueue");
    let recovered_delivery =
        tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
            .await
            .expect("recovered delivery timeout")
            .expect("recovered consumer ended")
            .expect("recovered delivery");
    assert_eq!(
        serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&recovered_delivery.data)
            .expect("decode recovered delivery")
            .invocation_id,
        after_disconnect.invocation_id
    );
    recovered_delivery
        .ack(BasicAckOptions::default())
        .await
        .expect("ack recovered delivery");
    assert!(dispatch.runtime_stats().publisher_connected);

    let recovered_publisher = dispatch
        .rabbitmq_publisher()
        .await
        .expect("load recovered publisher");
    let mut retry = envelope();
    retry.invocation_id = format!("retry-return-{suffix}");
    retry.delivery_attempt = 2;
    let retry_started = std::time::Instant::now();
    publish_envelope_to_queue(
        &recovered_publisher.channel,
        topology.rabbitmq_exchange.as_deref().unwrap(),
        topology.retry_queue_name.as_deref().unwrap(),
        &retry,
    )
    .await
    .expect("publish retry delivery");
    let retry_delivery = tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
        .await
        .expect("retry delivery timeout")
        .expect("retry consumer ended")
        .expect("retry delivery");
    assert!(retry_started.elapsed() >= std::time::Duration::from_millis(100));
    assert_eq!(
        serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&retry_delivery.data)
            .expect("decode retry delivery")
            .invocation_id,
        retry.invocation_id
    );
    retry_delivery
        .ack(BasicAckOptions::default())
        .await
        .expect("ack retry delivery");

    let mut exhausted = envelope();
    exhausted.invocation_id = format!("retry-exhausted-{suffix}");
    exhausted.delivery_attempt = topology.max_delivery_attempts;
    publish_envelope_to_queue(
        &recovered_publisher.channel,
        topology.rabbitmq_exchange.as_deref().unwrap(),
        topology.queue_name.as_deref().unwrap(),
        &exhausted,
    )
    .await
    .expect("publish exhausted delivery");
    let exhausted_delivery =
        tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
            .await
            .expect("exhausted delivery timeout")
            .expect("exhausted consumer ended")
            .expect("exhausted delivery");
    let state = AppState::new(crate::config::AppConfig::test())
        .await
        .expect("test state");
    settle_rabbitmq_delivery(
        &state,
        &topology,
        &consumer_channel,
        exhausted_delivery,
        exhausted.clone(),
        ProcessOutcome::Retry("forced acceptance failure".to_string()),
    )
    .await
    .expect("route exhausted delivery to DLQ");

    let queue_stats = dispatch.rabbitmq_queue_stats().await;
    assert!(queue_stats.enabled);
    assert!(queue_stats.available);
    assert_eq!(queue_stats.queues.len(), 3);
    assert!(queue_stats
        .queues
        .iter()
        .any(|queue| queue.role == "dispatch" && queue.consumers >= 1));
    assert!(queue_stats
        .queues
        .iter()
        .any(|queue| queue.role == "dead_letter" && queue.messages == 1));

    let mut dead_letter_consumer = consumer_channel
        .basic_consume(
            topology.dead_letter_queue_name.as_deref().unwrap(),
            format!("mcp-test-dlq-{suffix}").as_str(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("open DLQ consumer");
    let dead_letter = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        dead_letter_consumer.next(),
    )
    .await
    .expect("DLQ delivery timeout")
    .expect("DLQ consumer ended")
    .expect("DLQ delivery");
    let dead_letter_envelope =
        serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&dead_letter.data)
            .expect("decode DLQ delivery");
    assert_eq!(dead_letter_envelope.invocation_id, exhausted.invocation_id);
    assert_eq!(
        dead_letter_envelope.delivery_attempt,
        topology.max_delivery_attempts
    );
    dead_letter
        .ack(BasicAckOptions::default())
        .await
        .expect("ack DLQ delivery");

    for queue in [
        topology.queue_name.as_deref().unwrap(),
        topology.retry_queue_name.as_deref().unwrap(),
        topology.dead_letter_queue_name.as_deref().unwrap(),
    ] {
        consumer_channel
            .queue_delete(queue, lapin::options::QueueDeleteOptions::default())
            .await
            .expect("delete test queue");
    }
    consumer_channel
        .exchange_delete(
            topology.cancellation_exchange.as_deref().unwrap(),
            lapin::options::ExchangeDeleteOptions::default(),
        )
        .await
        .expect("delete test cancellation exchange");
    consumer_channel
        .exchange_delete(
            topology.rabbitmq_exchange.as_deref().unwrap(),
            lapin::options::ExchangeDeleteOptions::default(),
        )
        .await
        .expect("delete test exchange");
}

#[test]
fn dlq_archive_match_requires_full_invocation_identity_and_exhausted_attempt() {
    let mut record = RuntimeInvocationRecord {
        invocation_id: "invocation-1".to_string(),
        session_id: "session-1".to_string(),
        request_id_key: "request-1".to_string(),
        caller_service: "task-runner".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        project_id: "project-1".to_string(),
        device_id: None,
        resource_id: "resource-1".to_string(),
        exposed_tool_name: "tool-1".to_string(),
        original_tool_name: "tool-1".to_string(),
        mutation_may_have_started: false,
        cancel_supported: false,
        status: crate::runtime::RuntimeInvocationStatus::Failed,
        async_execution: true,
        created_at_unix_ms: 1,
        started_at_unix_ms: None,
        completed_at_unix_ms: Some(2),
        terminal_result: None,
        terminal_error_code: Some(-32603),
        terminal_error_message: Some("async tool dispatch failed after 5 attempts".to_string()),
        file_modification_outcome: None,
        result_reply_to: Some("mcp.results.test".to_string()),
        result_event_id: Some("event-1".to_string()),
        result_event_pending: false,
        expires_at: mongodb::bson::DateTime::from_millis(10_000),
        expires_at_unix: 10,
    };
    let payload = serde_json::to_vec(&QueuedAsyncToolCallEnvelope {
        delivery_attempt: 5,
        ..envelope()
    })
    .unwrap();
    assert!(async_tool_dead_letter_matches(&payload, &record, 5));
    record.resource_id = "resource-2".to_string();
    assert!(!async_tool_dead_letter_matches(&payload, &record, 5));
    assert!(!async_tool_dead_letter_matches(&payload, &record, 6));
}
