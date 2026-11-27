use super::*;

pub fn task_process_model_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
            "description": "Allocate or release a Run-scoped loopback service address before starting a local server. Allocate returns the actual port and service_url; pass that port to the backend and the returned URL to dependent frontend processes. A busy preferred_port is never killed or taken over and automatically falls back to another port.",
            "parameters": {
                "type": "object",
                "properties": {
                    "operation": {"type": "string", "enum": ["allocate", "release"], "default": "allocate"},
                    "service_name": {"type": "string", "minLength": 1, "maxLength": 64},
                    "preferred_port": {"type": "integer", "minimum": 1024, "maximum": 65535},
                    "scheme": {"type": "string", "enum": ["http", "https"], "default": "http"}
                },
                "required": ["service_name"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": TASK_PROCESS_RECORD_TOOL,
            "description": "Record a concise, user-visible Task execution milestone and any acceptance checks whose state changed. The Task detail UI shows the frozen criteria immediately and overlays these updates while the Run is active. Report concrete evidence as soon as a criterion becomes in_progress, passed, failed, or not_verified; do not wait for the final response. Once evidence fully satisfies a criterion, the next meaningful milestone must mark it passed instead of leaving it in_progress until the final report. Combine criteria changed by the same evidence in one update. Do not log every low-level tool call and never include secrets or hidden reasoning.",
            "parameters": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["append", "replace", "clear"],
                        "default": "append"
                    },
                    "heading": {"type": "string", "maxLength": 240},
                    "content": {"type": "string", "maxLength": PROCESS_ENTRY_MAX_CHARS},
                    "acceptance_checks": {
                        "type": "array",
                        "maxItems": 50,
                        "items": {
                            "type": "object",
                            "properties": {
                                "criterion_index": {"type":"integer", "minimum":0},
                                "status": {"type":"string", "enum":["pending","in_progress","passed","failed","not_verified","blocked"]},
                                "evidence": {"type":"string", "maxLength":2000}
                            },
                            "required":["criterion_index","status"],
                            "additionalProperties":false
                        }
                    }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": TASK_OUTCOME_REPORT_TOOL,
            "description": "Report a complete Task result and acceptance evidence. Successful completion is allowed only when result_status is succeeded and every frozen acceptance check is passed. Plan-bound blockers must be classified: handled_locally and retry_with_changed_method continue the same Task; replan_required returns the graph for a new planning revision; user_action_required is the only model-reported human wait. A planning Task with Requirement Survey must ask genuine product questions there and use replan_required for unavailable or inconsistent Task capabilities; it cannot stop as user_action_required. Unknown side effects are Host-owned needs_review and must never be claimed or replayed by the model. One blocked check does not waive the others. Corrected reports may be submitted after more work. Use this only when attempting to finish; a terminal report must be the final tool call and the Host delivers final_response unchanged.",
            "parameters": {
                "type": "object",
                "properties": {
                    "result_status": {
                        "type": "string",
                        "enum": ["succeeded", "failed", "blocked"],
                        "description": "Task result and dependency gate. succeeded means downstream prerequisites are satisfied; failed means the requested result did not meet its contract, but under acceptance-report v1 it does not terminate the Run while failed or unverified checks remain actionable; blocked means an external prerequisite or decision is still missing and is terminal only after every other acceptance check passes."
                    },
                    "final_response": {"type": "string", "minLength": 1, "maxLength": 32000, "description": "Exact user-facing final delivery, including all Markdown and whitespace; obey the frozen final_response_max_chars if present."},
                    "acceptance_checks": {
                        "type": "array", "maxItems": 50,
                        "items": {
                            "type": "object",
                            "properties": {
                                "criterion_index": {"type":"integer", "minimum":0},
                                "status": {"type":"string", "enum":["passed","failed","not_verified","blocked"]},
                                "evidence": {"type":"string", "minLength":1, "maxLength":2000}
                            },
                            "required":["criterion_index","status","evidence"],
                            "additionalProperties":false
                        }
                    },
                    "reason": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": OUTCOME_REASON_MAX_CHARS
                    },
                    "blocker": {
                        "type": "object",
                        "properties": {
                            "classification": {
                                "type": "string",
                                "enum": ["handled_locally", "retry_with_changed_method", "replan_required", "user_action_required", "unknown_side_effects_needs_review"]
                            },
                            "evidence": {"type":"string", "minLength":1, "maxLength":2000},
                            "failure_fingerprint": {"type":"string", "minLength":1, "maxLength":1000},
                            "previous_method": {"type":"string", "minLength":1, "maxLength":1000},
                            "next_method": {"type":"string", "minLength":1, "maxLength":1000},
                            "affected_task_ids": {
                                "type":"array", "maxItems":50, "uniqueItems":true,
                                "items":{"type":"string", "minLength":1, "maxLength":256}
                            },
                            "user_question": {"type":"string", "minLength":1, "maxLength":1000}
                        },
                        "required": ["classification", "evidence"],
                        "additionalProperties": false,
                        "description": "Required for a blocked plan-bound Task. retry_with_changed_method also requires fingerprint and distinct previous/next methods; replan_required requires affected_task_ids; user_action_required requires one concrete user_question."
                    },
                    "verification_report": {
                        "type": "object",
                        "properties": {
                            "subject_identity": {"type":"string", "minLength":1, "maxLength":512},
                            "full_rerun": {"type":"boolean"},
                            "checks": {
                                "type":"array", "minItems":1, "maxItems":50,
                                "items": {
                                    "type":"object",
                                    "properties": {
                                        "check_id": {"type":"string", "minLength":1, "maxLength":128},
                                        "status": {"type":"string", "enum":["passed","failed","skipped_due_to"]},
                                        "evidence": {"type":"string", "maxLength":2000},
                                        "skipped_due_to": {
                                            "type":"array", "maxItems":49, "uniqueItems":true,
                                            "items":{"type":"string", "minLength":1, "maxLength":128}
                                        }
                                    },
                                    "required":["check_id","status"],
                                    "additionalProperties":false
                                }
                            }
                        },
                        "required":["subject_identity","full_rerun","checks"],
                        "additionalProperties":false,
                        "description":"Required only when the Task has a verification_contract. Report every frozen check exactly once; skipped checks must name their failed or skipped direct dependencies."
                    }
                },
                "required": ["result_status", "reason"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn task_process_prompt_item() -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": format!(
                "[Task Execution Process]\nThe local run-scoped tools `{}`, `{}`, and `{}` are available for this Task. Before starting a local HTTP service, allocate its address with `{}`; pass the returned port to the server and service_url to every dependent process, and release the registration after the service stops. Keep the user-visible execution process updated at meaningful milestones: task start, approach or root cause, completion of a major phase or artifact, important verification results, changed path after failure, blockers, and next step. Whenever a frozen acceptance criterion changes state, include its zero-based criterion_index, status, and concrete evidence in `{}` so the Task detail UI updates while work is still running. When evidence fully proves a criterion, mark it passed in the next meaningful milestone; do not leave proven work in_progress until the final report, and combine criteria supported by the same evidence in one update. Do not record every tool call, file read, search, or edit. Keep entries concise and never record hidden reasoning, credentials, secrets, raw dumps, or unrelated drafts. Only after every acceptance criterion has concrete passed evidence, call `{}` with result_status=succeeded and one passed check per criterion. A technical problem is not automatically a user blocker: solve it locally, retry with a materially different safe method, or classify it replan_required. A plan-bound blocked report must include blocker. Use user_action_required only for credentials, permission, a product decision, or an external human action, with one concrete question. Never claim unknown_side_effects_needs_review; only the Host can establish it and the operation must not be replayed. A blocked criterion never waives other checks: continue working until every other criterion has passed, then report result_status=blocked with those criteria passed and only the externally blocked criteria marked blocked. If any actionable check is failed or not_verified, keep working and validating instead of finishing. If you still attempt to finish, the Host will return the actionable criteria and continue this Run automatically. A terminal outcome report must be the final tool call immediately before the user-facing final response.",
                TASK_PROCESS_RECORD_TOOL,
                TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
                TASK_OUTCOME_REPORT_TOOL,
                TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
                TASK_PROCESS_RECORD_TOOL,
                TASK_OUTCOME_REPORT_TOOL
            ) + " A structurally rejected report may be corrected without repeating external operations. Never claim passed without evidence."
        }]
    })
}
