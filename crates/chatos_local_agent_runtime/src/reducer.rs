// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{
    LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRun,
    LocalAgentRunStatus, ModelStepCompletion, ModelStepResult, ProtocolError,
    MAX_BOUNDED_JSON_BYTES,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducerPolicy {
    pub max_retries: u32,
}

impl Default for ReducerPolicy {
    fn default() -> Self {
        Self { max_retries: 3 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepEvidence {
    None,
    Model {
        result: ModelStepResult,
        pending_batch_id: Option<String>,
        retry_at: Option<DateTime<Utc>>,
    },
    ToolBatch {
        outcome_unknown: bool,
    },
}

impl From<ModelStepCompletion> for StepEvidence {
    fn from(completion: ModelStepCompletion) -> Self {
        Self::Model {
            result: completion.result,
            pending_batch_id: completion.pending_batch_id,
            retry_at: completion.retry_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmittedEvent {
    pub event_type: LocalAgentEventType,
    pub expected_version: u64,
    pub available_at: DateTime<Utc>,
    pub bounded_payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reduction {
    pub run: LocalAgentRun,
    pub emitted_events: Vec<EmittedEvent>,
    pub human_interaction: Option<HumanInteraction>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HumanInteraction {
    AskUser(Value),
    ReviewUnknownToolOutcome { batch_id: String },
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ReducerError {
    #[error("invalid local Agent contract: {0}")]
    InvalidContract(#[from] ProtocolError),
    #[error("event {event_id} does not belong to run {run_id}")]
    WrongRun { event_id: String, run_id: String },
    #[error("event expected run version {expected}; actual version is {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("event must be claimed before reduction")]
    EventNotClaimed,
    #[error("{event_type:?} cannot run while status is {status:?}")]
    InvalidTransition {
        event_type: LocalAgentEventType,
        status: LocalAgentRunStatus,
    },
    #[error("{event_type:?} received incompatible step evidence")]
    InvalidEvidence { event_type: LocalAgentEventType },
    #[error("run counter overflow")]
    CounterOverflow,
}

pub fn reduce_claimed_event(
    run: &LocalAgentRun,
    event: &LocalAgentEvent,
    evidence: StepEvidence,
    now: DateTime<Utc>,
    policy: ReducerPolicy,
) -> Result<Reduction, ReducerError> {
    run.validate()?;
    event.validate()?;
    if event.run_id != run.run_id {
        return Err(ReducerError::WrongRun {
            event_id: event.event_id.clone(),
            run_id: run.run_id.clone(),
        });
    }
    if event.expected_version != run.version {
        return Err(ReducerError::VersionConflict {
            expected: event.expected_version,
            actual: run.version,
        });
    }
    if event.status != LocalAgentEventStatus::Claimed {
        return Err(ReducerError::EventNotClaimed);
    }
    if run.status.is_terminal() && event.event_type != LocalAgentEventType::RunTerminal {
        return invalid_transition(run, event);
    }

    let mut next = run.clone();
    next.version = next
        .version
        .checked_add(1)
        .ok_or(ReducerError::CounterOverflow)?;
    next.updated_at = now;
    let mut emitted_events = Vec::new();
    let mut human_interaction = None;

    match event.event_type {
        LocalAgentEventType::RunStarted => {
            require_status(run, event, &[LocalAgentRunStatus::Queued])?;
            require_no_evidence(event, &evidence)?;
            next.status = LocalAgentRunStatus::ModelReady;
            next.pending_interaction = None;
            emit(
                &mut emitted_events,
                &next,
                LocalAgentEventType::ModelStepRequested,
                now,
                Value::Null,
            );
        }
        LocalAgentEventType::ModelStepRequested => {
            require_status(run, event, &[LocalAgentRunStatus::ModelReady])?;
            require_no_evidence(event, &evidence)?;
            next.status = LocalAgentRunStatus::ModelRunning;
            next.step_seq = next
                .step_seq
                .checked_add(1)
                .ok_or(ReducerError::CounterOverflow)?;
        }
        LocalAgentEventType::ModelStepCompleted => {
            require_status(run, event, &[LocalAgentRunStatus::ModelRunning])?;
            reduce_model_result(
                &mut next,
                event,
                evidence,
                now,
                policy,
                &mut emitted_events,
                &mut human_interaction,
            )?;
        }
        LocalAgentEventType::ToolBatchRequested => {
            require_status(run, event, &[LocalAgentRunStatus::WaitingToolResult])?;
            require_no_evidence(event, &evidence)?;
        }
        LocalAgentEventType::ToolBatchCompleted => {
            require_status(run, event, &[LocalAgentRunStatus::WaitingToolResult])?;
            let StepEvidence::ToolBatch { outcome_unknown } = evidence else {
                return Err(ReducerError::InvalidEvidence {
                    event_type: event.event_type,
                });
            };
            next.pending_batch_id = None;
            if outcome_unknown {
                next.status = LocalAgentRunStatus::NeedsReview;
                human_interaction = Some(HumanInteraction::ReviewUnknownToolOutcome {
                    batch_id: run.pending_batch_id.clone().ok_or(
                        ReducerError::InvalidTransition {
                            event_type: event.event_type,
                            status: run.status,
                        },
                    )?,
                });
                next.pending_interaction = Some(json!({
                    "type": "review_unknown_tool_outcome",
                    "batch_id": run.pending_batch_id,
                }));
            } else {
                next.status = LocalAgentRunStatus::ContinuationReady;
                emit(
                    &mut emitted_events,
                    &next,
                    LocalAgentEventType::ContinuationRequested,
                    now,
                    Value::Null,
                );
            }
        }
        LocalAgentEventType::ContinuationRequested | LocalAgentEventType::RetryDue => {
            let allowed = if event.event_type == LocalAgentEventType::ContinuationRequested {
                LocalAgentRunStatus::ContinuationReady
            } else {
                LocalAgentRunStatus::RetryScheduled
            };
            require_status(run, event, &[allowed])?;
            require_no_evidence(event, &evidence)?;
            next.status = LocalAgentRunStatus::ModelReady;
            next.pending_interaction = None;
            emit(
                &mut emitted_events,
                &next,
                LocalAgentEventType::ModelStepRequested,
                now,
                Value::Null,
            );
        }
        LocalAgentEventType::PauseRequested => {
            require_nonterminal(run, event)?;
            require_no_evidence(event, &evidence)?;
            next.status = LocalAgentRunStatus::Paused;
        }
        LocalAgentEventType::ResumeRequested => {
            require_status(
                run,
                event,
                &[
                    LocalAgentRunStatus::Paused,
                    LocalAgentRunStatus::NeedsReview,
                ],
            )?;
            require_no_evidence(event, &evidence)?;
            next.status = LocalAgentRunStatus::ModelReady;
            next.pending_interaction = None;
            emit(
                &mut emitted_events,
                &next,
                LocalAgentEventType::ModelStepRequested,
                now,
                Value::Null,
            );
        }
        LocalAgentEventType::CancelRequested => {
            require_nonterminal(run, event)?;
            require_no_evidence(event, &evidence)?;
            set_terminal(
                &mut next,
                LocalAgentRunStatus::Cancelled,
                json!({"reason": "cancel_requested"}),
            );
            emit_terminal(&mut emitted_events, &next, now);
        }
        LocalAgentEventType::MemorySyncDue => {
            require_nonterminal(run, event)?;
            require_no_evidence(event, &evidence)?;
        }
        LocalAgentEventType::RunTerminal => {
            if !run.status.is_terminal() {
                return invalid_transition(run, event);
            }
            require_no_evidence(event, &evidence)?;
        }
    }

    validate_emitted_events(&emitted_events)?;
    next.validate()?;
    Ok(Reduction {
        run: next,
        emitted_events,
        human_interaction,
    })
}

fn reduce_model_result(
    next: &mut LocalAgentRun,
    event: &LocalAgentEvent,
    evidence: StepEvidence,
    now: DateTime<Utc>,
    policy: ReducerPolicy,
    emitted: &mut Vec<EmittedEvent>,
    human_interaction: &mut Option<HumanInteraction>,
) -> Result<(), ReducerError> {
    let StepEvidence::Model {
        result,
        pending_batch_id,
        retry_at,
    } = evidence
    else {
        return Err(ReducerError::InvalidEvidence {
            event_type: event.event_type,
        });
    };
    next.iteration = next
        .iteration
        .checked_add(1)
        .ok_or(ReducerError::CounterOverflow)?;
    match result {
        ModelStepResult::ToolCommand(payload) => {
            let Some(batch_id) = pending_batch_id.filter(|value| !value.trim().is_empty()) else {
                return Err(ReducerError::InvalidEvidence {
                    event_type: event.event_type,
                });
            };
            next.pending_batch_id = Some(batch_id);
            next.status = LocalAgentRunStatus::WaitingToolResult;
            emit(
                emitted,
                next,
                LocalAgentEventType::ToolBatchRequested,
                now,
                payload,
            );
        }
        ModelStepResult::Continue(payload) => {
            next.status = LocalAgentRunStatus::ContinuationReady;
            emit(
                emitted,
                next,
                LocalAgentEventType::ContinuationRequested,
                now,
                payload,
            );
        }
        ModelStepResult::Retry(payload) => {
            next.retry_count = next
                .retry_count
                .checked_add(1)
                .ok_or(ReducerError::CounterOverflow)?;
            if next.retry_count > policy.max_retries {
                set_terminal(
                    next,
                    LocalAgentRunStatus::Failed,
                    json!({"reason": "retry_limit_exceeded"}),
                );
                emit_terminal(emitted, next, now);
            } else {
                let Some(retry_at) = retry_at else {
                    return Err(ReducerError::InvalidEvidence {
                        event_type: event.event_type,
                    });
                };
                next.status = LocalAgentRunStatus::RetryScheduled;
                emit(
                    emitted,
                    next,
                    LocalAgentEventType::RetryDue,
                    retry_at,
                    payload,
                );
            }
        }
        ModelStepResult::AskUser(question) => {
            next.status = LocalAgentRunStatus::Paused;
            next.pending_interaction = Some(json!({
                "type": "ask_user",
                "question": question.clone(),
            }));
            *human_interaction = Some(HumanInteraction::AskUser(question));
        }
        ModelStepResult::Final(outcome) => {
            set_terminal(next, LocalAgentRunStatus::Succeeded, outcome);
            emit_terminal(emitted, next, now);
        }
        ModelStepResult::Failed(outcome) => {
            set_terminal(next, LocalAgentRunStatus::Failed, outcome);
            emit_terminal(emitted, next, now);
        }
        ModelStepResult::Cancelled => {
            set_terminal(
                next,
                LocalAgentRunStatus::Cancelled,
                json!({"reason": "model_cancelled"}),
            );
            emit_terminal(emitted, next, now);
        }
    }
    Ok(())
}

fn require_status(
    run: &LocalAgentRun,
    event: &LocalAgentEvent,
    allowed: &[LocalAgentRunStatus],
) -> Result<(), ReducerError> {
    if allowed.contains(&run.status) {
        Ok(())
    } else {
        invalid_transition(run, event)
    }
}

fn require_nonterminal(run: &LocalAgentRun, event: &LocalAgentEvent) -> Result<(), ReducerError> {
    if run.status.is_terminal() {
        invalid_transition(run, event)
    } else {
        Ok(())
    }
}

fn require_no_evidence(
    event: &LocalAgentEvent,
    evidence: &StepEvidence,
) -> Result<(), ReducerError> {
    if matches!(evidence, StepEvidence::None) {
        Ok(())
    } else {
        Err(ReducerError::InvalidEvidence {
            event_type: event.event_type,
        })
    }
}

fn invalid_transition<T>(run: &LocalAgentRun, event: &LocalAgentEvent) -> Result<T, ReducerError> {
    Err(ReducerError::InvalidTransition {
        event_type: event.event_type,
        status: run.status,
    })
}

fn set_terminal(run: &mut LocalAgentRun, status: LocalAgentRunStatus, outcome: Value) {
    run.status = status;
    run.pending_batch_id = None;
    run.pending_interaction = None;
    run.terminal_outcome = Some(outcome);
}

fn emit(
    target: &mut Vec<EmittedEvent>,
    run: &LocalAgentRun,
    event_type: LocalAgentEventType,
    available_at: DateTime<Utc>,
    bounded_payload: Value,
) {
    target.push(EmittedEvent {
        event_type,
        expected_version: run.version,
        available_at,
        bounded_payload,
    });
}

fn emit_terminal(target: &mut Vec<EmittedEvent>, run: &LocalAgentRun, now: DateTime<Utc>) {
    emit(
        target,
        run,
        LocalAgentEventType::RunTerminal,
        now,
        Value::Null,
    );
}

fn validate_emitted_events(events: &[EmittedEvent]) -> Result<(), ReducerError> {
    for event in events {
        let bytes = serde_json::to_vec(&event.bounded_payload).map_err(|_| {
            ReducerError::InvalidContract(ProtocolError::InvalidJson {
                field: "emitted_event_payload",
            })
        })?;
        if bytes.len() > MAX_BOUNDED_JSON_BYTES {
            return Err(ReducerError::InvalidContract(
                ProtocolError::PayloadTooLarge {
                    field: "emitted_event_payload",
                    bytes: bytes.len(),
                    maximum: MAX_BOUNDED_JSON_BYTES,
                },
            ));
        }
    }
    Ok(())
}
