use lapin::{publisher_confirm::Confirmation, types::AMQPValue};

use chatos_mcp_service::{McpToolCallCommand, McpToolCallCommandItem};

use super::*;

fn command(call_count: usize) -> McpToolCallCommand {
    McpToolCallCommand {
        owner_service: "task-runner".to_string(),
        agent_run_id: "run-1".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        ordering_lane_key: "task:task-1".to_string(),
        lane_seq: 1,
        generation: 1,
        source_step_seq: 1,
        batch_id: "batch-1".to_string(),
        mcp_runtime_session_ref: "session-1".to_string(),
        result_routing_key: "cloud_agent.task_runner.mcp_results".to_string(),
        calls: (0..call_count)
            .map(|call_index| McpToolCallCommandItem {
                invocation_id: format!("invocation-{call_index}"),
                tool_call_id: format!("tool-call-{call_index}"),
                call_index,
                name: "project.read".to_string(),
                arguments: serde_json::json!({}),
                preflight_error: None,
            })
            .collect(),
        delivery_attempt: 1,
    }
}

#[test]
fn single_and_multiple_tool_calls_share_the_same_command_contract() {
    let single = command(1);
    let batch = command(3);

    assert_eq!(single.calls.len(), 1);
    assert_eq!(batch.calls.len(), 3);
    assert_eq!(
        serde_json::to_value(&single).unwrap().get("calls").unwrap()[0]["call_index"],
        0
    );
    assert_eq!(
        serde_json::to_value(&batch).unwrap().get("calls").unwrap()[2]["call_index"],
        2
    );
}

#[test]
fn command_retry_is_bounded_and_preserves_all_calls() {
    let first = command(3);
    let second = first.next_retry(3).expect("second attempt");
    let third = second.next_retry(3).expect("third attempt");

    assert_eq!(second.delivery_attempt, 2);
    assert_eq!(third.delivery_attempt, 3);
    assert_eq!(third.calls, first.calls);
    assert!(third.next_retry(3).is_none());
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
    let error = ensure_publish_confirmed("mcp.commands", Confirmation::Nack(None))
        .expect_err("publisher nack must fail");
    assert_eq!(error, AsyncToolEnqueueError::CapacityExhausted);
}
