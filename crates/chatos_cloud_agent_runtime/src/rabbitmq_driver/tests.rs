// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn topology_requires_distinct_durable_queue_identities() {
    let mut topology = CloudAgentRabbitMqTopology {
        rabbitmq_url: "amqp://localhost".to_string(),
        exchange: "cloud_agent".to_string(),
        runtime_queue: "cloud_agent.project.runtime".to_string(),
        retry_queue: "cloud_agent.project.runtime.retry".to_string(),
        consumer_tag: "project-cloud-agent".to_string(),
        reconnect_delay: Duration::from_secs(1),
        outbox_reconcile_interval: Duration::from_secs(1),
        outbox_batch_size: 100,
        prefetch_count: 32,
        consumer_concurrency: 4,
        conflict_retry_delay: Duration::from_secs(1),
    };
    assert!(topology.validate().is_ok());
    topology.consumer_concurrency = 0;
    assert!(topology.validate().is_err());
    topology.consumer_concurrency = 4;
    topology.outbox_reconcile_interval = Duration::ZERO;
    assert!(topology.validate().is_err());
}

#[test]
fn delivery_attempt_defaults_to_one_and_reads_retry_header() {
    assert_eq!(cloud_agent_delivery_attempt(&BasicProperties::default()), 1);
    let properties = BasicProperties::default().with_headers(cloud_agent_delivery_headers(4, None));
    assert_eq!(cloud_agent_delivery_attempt(&properties), 4);
}

#[test]
fn deleted_owner_entities_are_consumed_as_stale() {
    for error in [
        "Cloud Agent run not found: run-1",
        "Task Run not found: run-1",
        "Task not found: task-1",
        "parent Task Run not found: run-1",
        "parent Cloud Agent run not found",
    ] {
        assert!(cloud_agent_delivery_error_is_stale(error));
    }
    assert!(!cloud_agent_delivery_error_is_stale(
        "Cloud Agent lifecycle arrived before terminal state"
    ));
}

#[test]
fn delivery_failure_header_is_bounded() {
    assert_eq!(truncate_delivery_failure(&"x".repeat(2_000)).len(), 1_024);
}

#[test]
fn amqp_property_ids_are_utf8_safe_stable_and_bounded() {
    let short = "event-1";
    assert_eq!(bounded_amqp_property_id(short), short);

    let long = format!("event:{}", "任务".repeat(120));
    let bounded = bounded_amqp_property_id(long.as_str());
    assert!(bounded.len() <= MAX_AMQP_SHORT_STRING_BYTES);
    assert_eq!(bounded, bounded_amqp_property_id(long.as_str()));
    assert_ne!(
        bounded,
        bounded_amqp_property_id(format!("{long}-different").as_str())
    );
    assert!(bounded.contains('#'));
}

#[test]
fn outbox_publish_retry_delay_is_exponential_and_capped() {
    assert_eq!(outbox_publish_retry_delay(1), Duration::from_secs(1));
    assert_eq!(outbox_publish_retry_delay(2), Duration::from_secs(2));
    assert_eq!(outbox_publish_retry_delay(8), Duration::from_secs(128));
    assert_eq!(outbox_publish_retry_delay(20), Duration::from_secs(300));
}

#[test]
fn outbox_reconcile_startup_jitter_is_stable_and_bounded() {
    let interval = Duration::from_secs(5);
    let jitter = outbox_reconcile_startup_jitter(interval, "task-runner:worker-1");
    assert_eq!(
        jitter,
        outbox_reconcile_startup_jitter(interval, "task-runner:worker-1")
    );
    assert!(jitter <= Duration::from_secs(1));
    assert_eq!(
        outbox_reconcile_startup_jitter(Duration::from_millis(3), "worker"),
        Duration::ZERO
    );
}

#[test]
fn processing_error_retry_delay_is_exponential_and_capped() {
    let base = Duration::from_secs(1);
    assert_eq!(cloud_agent_retry_delay(base, 2), Duration::from_secs(1));
    assert_eq!(cloud_agent_retry_delay(base, 3), Duration::from_secs(2));
    assert_eq!(cloud_agent_retry_delay(base, 8), Duration::from_secs(60));
}
