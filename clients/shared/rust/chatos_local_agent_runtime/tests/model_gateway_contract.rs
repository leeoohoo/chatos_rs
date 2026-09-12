// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{
    ModelGatewayStreamEnvelope, ModelGatewayStreamEvent, ModelGatewayTerminal,
    ModelGatewayTerminalSource, ModelGatewayTerminalStatus, ModelProtocol,
};
use chatos_local_agent_runtime::{ModelGatewayStreamAccumulator, ModelGatewayStreamError};

fn envelope(sequence: u64, event: ModelGatewayStreamEvent) -> ModelGatewayStreamEnvelope {
    ModelGatewayStreamEnvelope {
        request_id: "request-1".to_string(),
        sequence,
        protocol: ModelProtocol::Responses,
        event,
    }
}

fn terminal(status: ModelGatewayTerminalStatus) -> ModelGatewayTerminal {
    ModelGatewayTerminal {
        status,
        source: ModelGatewayTerminalSource::Provider,
        response_id: Some("resp_1".to_string()),
        provider_request_id: Some("req_1".to_string()),
        terminal_event: match status {
            ModelGatewayTerminalStatus::Completed => "response.completed",
            ModelGatewayTerminalStatus::Incomplete => "response.incomplete",
            ModelGatewayTerminalStatus::Failed => "response.failed",
        }
        .to_string(),
        provider_http_status: Some(200),
        usage: Some(serde_json::json!({"input_tokens": 10, "output_tokens": 2})),
        output_items: vec![serde_json::json!({"type": "message", "id": "msg_1"})],
        incomplete_details: (status == ModelGatewayTerminalStatus::Incomplete)
            .then(|| serde_json::json!({"reason": "max_output_tokens"})),
        provider_error: (status == ModelGatewayTerminalStatus::Failed)
            .then(|| serde_json::json!({"code": "provider_failed"})),
    }
}

#[test]
fn deltas_and_output_items_require_a_formal_terminal_event() {
    let mut accumulator = ModelGatewayStreamAccumulator::new("request-1", ModelProtocol::Responses);
    accumulator
        .accept(envelope(
            1,
            ModelGatewayStreamEvent::ContentDelta {
                delta: "Hello".to_string(),
            },
        ))
        .unwrap();
    assert_eq!(
        accumulator.finish(),
        Err(ModelGatewayStreamError::MissingTerminal)
    );
}

#[test]
fn completed_stream_preserves_terminal_identity_usage_and_output_items() {
    let output_item = serde_json::json!({"type": "message", "id": "msg_1"});
    let mut accumulator = ModelGatewayStreamAccumulator::new("request-1", ModelProtocol::Responses);
    accumulator
        .accept(envelope(
            1,
            ModelGatewayStreamEvent::ReasoningDelta {
                delta: "Think".to_string(),
            },
        ))
        .unwrap();
    accumulator
        .accept(envelope(
            2,
            ModelGatewayStreamEvent::ContentDelta {
                delta: "Done".to_string(),
            },
        ))
        .unwrap();
    accumulator
        .accept(envelope(
            3,
            ModelGatewayStreamEvent::OutputItem {
                item: output_item.clone(),
            },
        ))
        .unwrap();
    accumulator
        .accept(envelope(
            4,
            ModelGatewayStreamEvent::Terminal {
                terminal: Box::new(terminal(ModelGatewayTerminalStatus::Completed)),
            },
        ))
        .unwrap();

    let output = accumulator.finish().unwrap();
    assert_eq!(output.content, "Done");
    assert_eq!(output.reasoning, "Think");
    assert_eq!(output.output_items, vec![output_item]);
    assert_eq!(output.terminal.response_id.as_deref(), Some("resp_1"));
    assert_eq!(
        output.terminal.provider_request_id.as_deref(),
        Some("req_1")
    );
    assert!(output.terminal.usage.is_some());
}

#[test]
fn gaps_request_changes_and_events_after_terminal_are_rejected() {
    let mut accumulator = ModelGatewayStreamAccumulator::new("request-1", ModelProtocol::Responses);
    assert_eq!(
        accumulator.accept(envelope(
            2,
            ModelGatewayStreamEvent::ContentDelta {
                delta: "gap".to_string(),
            },
        )),
        Err(ModelGatewayStreamError::SequenceMismatch {
            expected: 1,
            actual: 2,
        })
    );

    let mut wrong_request = envelope(
        1,
        ModelGatewayStreamEvent::ContentDelta {
            delta: "wrong".to_string(),
        },
    );
    wrong_request.request_id = "request-2".to_string();
    assert!(matches!(
        accumulator.accept(wrong_request),
        Err(ModelGatewayStreamError::RequestMismatch { .. })
    ));

    accumulator
        .accept(envelope(
            1,
            ModelGatewayStreamEvent::Terminal {
                terminal: Box::new(terminal(ModelGatewayTerminalStatus::Completed)),
            },
        ))
        .unwrap();
    assert_eq!(
        accumulator.accept(envelope(
            2,
            ModelGatewayStreamEvent::ContentDelta {
                delta: "late".to_string(),
            },
        )),
        Err(ModelGatewayStreamError::EventAfterTerminal)
    );
}

#[test]
fn incomplete_and_failed_are_formal_terminal_results_not_empty_successes() {
    for status in [
        ModelGatewayTerminalStatus::Incomplete,
        ModelGatewayTerminalStatus::Failed,
    ] {
        let mut accumulator =
            ModelGatewayStreamAccumulator::new("request-1", ModelProtocol::Responses);
        accumulator
            .accept(envelope(
                1,
                ModelGatewayStreamEvent::Terminal {
                    terminal: Box::new(terminal(status)),
                },
            ))
            .unwrap();
        assert_eq!(accumulator.finish().unwrap().terminal.status, status);
    }
}
