use super::runtime::{Session, Trace, OWNER, SKILL};
use chatos_local_agent_protocol::{
    local_task_runtime_write_scope, local_task_write_scope_overlap, LocalAgentToolStatus,
    LocalTaskGraph, LocalTaskRecord, LocalTaskStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    pub schema_version: u32,
    pub capability_profiles: BTreeMap<String, Capabilities>,
    pub scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub builtins: Vec<String>,
    pub plugins: Vec<String>,
    pub external: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    pub category: String,
    pub domain: String,
    pub capabilities: String,
    pub prompt: String,
    #[serde(default)]
    pub expect: Expectations,
    pub seed: Option<Value>,
    pub rubric: Vec<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub min_tasks: Option<usize>,
    pub max_tasks: Option<usize>,
    pub handoffs: Option<usize>,
    #[serde(default)]
    pub synthesis: bool,
    pub followup_action: Option<String>,
    #[serde(default)]
    pub supersedes: bool,
}
impl Dataset {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        let mut counts = BTreeMap::new();
        for case in &self.scenarios {
            if case.id.is_empty()
                || !ids.insert(&case.id)
                || case.prompt.trim().is_empty()
                || case.rubric.is_empty()
                || case.rubric.iter().any(|item| item.trim().is_empty())
                || !self.capability_profiles.contains_key(&case.capabilities)
                || ![
                    "software-engineering",
                    "design-and-creative",
                    "writing-content-and-marketing",
                    "research-data-and-operations",
                    "documents-presentations-and-media",
                    "capabilities-and-plugins",
                ]
                .contains(&case.domain.as_str())
                || (case.category == "followup") != case.seed.is_some()
            {
                return Err("invalid_eval_scenario".into());
            }
            let (min, max, _) = case.bounds();
            if min > max || max > 50 {
                return Err("invalid_eval_expectation".into());
            }
            if case
                .expect
                .followup_action
                .as_deref()
                .is_some_and(|s| !["continue", "cancel", "clarify"].contains(&s))
            {
                return Err("invalid_eval_followup_action".into());
            }
            *counts.entry(case.category.as_str()).or_insert(0) += 1;
        }
        if self.schema_version != 1
            || counts
                != BTreeMap::from([
                    ("answer", 10),
                    ("single", 10),
                    ("graph", 20),
                    ("followup", 10),
                ])
        {
            return Err("evaluation_requires_50_balanced_scenarios".into());
        }
        Ok(())
    }
}
impl Scenario {
    pub fn bounds(&self) -> (usize, usize, usize) {
        let defaults = match self.category.as_str() {
            "answer" => (0, 0, 0),
            "single" => (1, 1, 1),
            "graph" => (2, 7, 1),
            _ => (0, 0, 1),
        };
        (
            self.expect.min_tasks.unwrap_or(defaults.0),
            self.expect.max_tasks.unwrap_or(defaults.1),
            self.expect.handoffs.unwrap_or(defaults.2),
        )
    }
}

#[derive(Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}
fn check(out: &mut Vec<Check>, name: &str, passed: bool, detail: impl Into<String>) {
    out.push(Check {
        name: name.into(),
        passed,
        detail: detail.into(),
    });
}
fn success(trace: &Trace) -> bool {
    trace.status == LocalAgentToolStatus::Succeeded
}

fn recovered_error(traces: &[Trace], index: usize) -> bool {
    let failed = &traces[index];
    if failed.status != LocalAgentToolStatus::Failed
        || !failed.result.as_ref().is_some_and(|result| {
            result["side_effects_started"] == false
                && ["task_preparation", "task_graph_validation"]
                    .contains(&result["phase"].as_str().unwrap_or(""))
        })
    {
        return false;
    }
    let creates = ["create_task", "create_tasks_with_prerequisites"];
    traces[index + 1..]
        .iter()
        .take_while(|trace| trace.tool_name != "wait_for_task_completion")
        .any(|trace| {
            trace.step > failed.step
                && success(trace)
                && (trace.tool_name == failed.tool_name
                    || creates.contains(&failed.tool_name.as_str())
                        && creates.contains(&trace.tool_name.as_str()))
        })
}
fn strings(value: &Value) -> Option<Vec<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}
pub fn contracts_valid(tasks: &[LocalTaskRecord]) -> bool {
    let mut outputs = BTreeSet::new();
    tasks.iter().all(|task| {
        task.input["objective"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
            && ["expected_outputs", "acceptance_criteria"]
                .iter()
                .all(|field| {
                    strings(&task.input[*field]).is_some_and(|items| {
                        !items.is_empty()
                            && items.iter().all(|s| !s.trim().is_empty())
                            && items.iter().collect::<BTreeSet<_>>().len() == items.len()
                            && (*field != "expected_outputs"
                                || items
                                    .iter()
                                    .all(|s| outputs.insert(s.trim().to_lowercase())))
                    })
                })
    })
}
pub fn capabilities_valid(tasks: &[LocalTaskRecord], allowed: &Capabilities) -> bool {
    tasks.iter().all(|task| {
        let options = &task.input["tool_options"];
        strings(&options["enabled_builtin_kinds"]).is_some_and(|items| {
            items
                .iter()
                .all(|s| allowed.builtins.iter().any(|v| v == s))
        }) && strings(&options["external_mcp_config_ids"]).is_some_and(|items| {
            items
                .iter()
                .all(|s| allowed.external.iter().any(|v| v == s))
        }) && options["plugin_hints"].as_array().is_some_and(|items| {
            items.iter().all(|hint| {
                hint["plugin_key"]
                    .as_str()
                    .is_some_and(|s| allowed.plugins.iter().any(|v| v == s))
                    && hint["component_key"]
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty())
            })
        })
    })
}
fn reaches(graph: &LocalTaskGraph, prerequisite: &str, dependent: &str) -> bool {
    let mut pending = vec![dependent];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if id == prerequisite {
            return true;
        }
        if seen.insert(id) {
            pending.extend(
                graph
                    .dependencies
                    .iter()
                    .filter(|edge| edge.task_id == id)
                    .map(|e| e.prerequisite_task_id.as_str()),
            );
        }
    }
    false
}
fn graph_has_synthesis(graph: &LocalTaskGraph) -> bool {
    graph.tasks.len() >= 2
        && graph.tasks.iter().any(|sink| {
            graph
                .tasks
                .iter()
                .all(|source| reaches(graph, &source.task_id, &sink.task_id))
        })
}
fn safe_active_writers(tasks: &[LocalTaskRecord]) -> bool {
    let active = tasks
        .iter()
        .filter(|task| {
            task.active_run_id.is_some()
                && matches!(
                    task.status,
                    LocalTaskStatus::Ready | LocalTaskStatus::Running
                )
        })
        .collect::<Vec<_>>();
    for (index, left) in active.iter().enumerate() {
        for right in &active[index + 1..] {
            match (
                local_task_runtime_write_scope(&left.input, &left.task_id),
                local_task_runtime_write_scope(&right.input, &right.task_id),
            ) {
                (Ok(Some(a)), Ok(Some(b))) if local_task_write_scope_overlap(&a, &b).is_some() => {
                    return false
                }
                (Err(_), _) | (_, Err(_)) => return false,
                _ => {}
            }
        }
    }
    true
}

// Coverage is measured from successful, durable results, not requested filenames.
// Require all character ranges before authoring contracts; a one-character read is not disclosure.
fn resource_read_before(traces: &[Trace], path: &str, end: usize) -> bool {
    let mut ranges = Vec::new();
    let mut total = None;
    for trace in &traces[..end] {
        if trace.tool_name != "agent_skill_read_resource" || !success(trace) {
            continue;
        }
        let Some(result) = &trace.result else {
            continue;
        };
        if result["skill_ref"] != SKILL || result["relative_path"] != path {
            continue;
        }
        let (Some(start), Some(length), Some(size)) = (
            result["offset"].as_u64(),
            result["returned_chars"].as_u64(),
            result["total_chars"].as_u64(),
        ) else {
            continue;
        };
        if total.is_some_and(|old| old != size) {
            return false;
        }
        total = Some(size);
        ranges.push((start, start.saturating_add(length)));
    }
    ranges.sort_unstable();
    let mut covered = 0;
    for (start, end) in ranges {
        if start > covered {
            return false;
        }
        covered = covered.max(end);
    }
    total.is_some_and(|size| size > 0 && covered >= size)
}

fn preceding_model_results(traces: &[Trace], decision: Option<usize>) -> usize {
    decision.map_or(traces.len(), |index| {
        traces
            .iter()
            .position(|trace| trace.step >= traces[index].step)
            .unwrap_or(index)
    })
}

#[derive(Serialize)]
pub struct CaseReport {
    pub id: String,
    pub category: String,
    pub structural_pass: bool,
    pub semantic_review_required: bool,
    pub semantic_status: &'static str,
    pub rubric: Vec<String>,
    pub checks: Vec<Check>,
    pub requests: usize,
    pub recovered_tool_errors: usize,
    pub failure: Option<String>,
    pub final_text: Option<String>,
    pub seed_task_ids: Vec<String>,
    pub tasks: Vec<LocalTaskRecord>,
    pub graphs: Vec<LocalTaskGraph>,
    pub traces: Vec<Trace>,
}

pub async fn report(
    case: &Scenario,
    allowed: &Capabilities,
    mut session: Session,
) -> Result<CaseReport, String> {
    let tasks = session.tasks().await?;
    let measured = tasks
        .iter()
        .filter(|task| !session.seed_ids.contains(&task.task_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut graphs = Vec::new();
    for id in measured
        .iter()
        .map(|t| &t.graph_id)
        .collect::<BTreeSet<_>>()
    {
        graphs.push(
            session
                .runtime
                .task_graph_by_id(OWNER, id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("missing_durable_graph")?,
        );
    }
    let mut checks = Vec::new();
    let (min, max, handoffs) = case.bounds();
    check(
        &mut checks,
        "planning_completed",
        session.failure.is_none()
            && session
                .final_text
                .as_ref()
                .is_some_and(|s| !s.trim().is_empty()),
        "Main Chat must finish with a user-facing response",
    );
    check(
        &mut checks,
        "task_count",
        (min..=max).contains(&measured.len()),
        format!(
            "{} new Tasks, expected {min}..={max}; seed phase excluded",
            measured.len()
        ),
    );
    check(&mut checks, "task_contracts", contracts_valid(&measured), "Nonempty contracts and globally distinct output labels; semantic specificity requires review");
    check(
        &mut checks,
        "capability_allowlist",
        capabilities_valid(&measured, allowed),
        "Persisted selections within scenario allowlist; exact minimality requires review",
    );
    let recovered_tool_errors = session
        .traces
        .iter()
        .enumerate()
        .filter(|(index, _)| recovered_error(&session.traces, *index))
        .count();
    check(
        &mut checks,
        "tool_results_safe",
        session.traces.iter().enumerate().all(|(index, trace)| success(trace) || recovered_error(&session.traces, index)),
        format!("{recovered_tool_errors} known pre-write errors corrected in a later step; unknown results and unrecovered errors fail"),
    );
    let handoff_indices = session
        .traces
        .iter()
        .enumerate()
        .filter(|(_, t)| t.tool_name == "wait_for_task_completion" && success(t))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    check(
        &mut checks,
        "handoff_count",
        handoff_indices.len() == handoffs,
        format!(
            "{} successful handoffs, expected {handoffs}",
            handoff_indices.len()
        ),
    );
    check(
        &mut checks,
        "handoff_terminal",
        handoff_indices.first().is_none_or(|index| {
            session.traces[index + 1..]
                .iter()
                .all(|t| !chatos_local_agent_host::TASK_TOOL_NAMES.contains(&t.tool_name.as_str()))
        }),
        "No Task tools after handoff",
    );
    let authored = session.traces.iter().position(|t| {
        ["create_task", "create_tasks_with_prerequisites"].contains(&t.tool_name.as_str())
            && success(t)
    });
    let authored_end = preceding_model_results(&session.traces, authored);
    let gate = authored.is_none_or(|_| {
        !session.seed_ids.is_empty()
            || session.traces[..authored_end].iter().any(|t| {
                t.tool_name == "agent_skill_activate"
                    && success(t)
                    && t.result
                        .as_ref()
                        .is_some_and(|r| r["activated"] == true && r["skill_ref"] == SKILL)
            })
    });
    check(&mut checks, "skill_activation", gate, "Successful orchestration activation before creation (seed activation retained for followups)");
    let mut required = Vec::new();
    if authored.is_some() {
        required.push((format!("references/{}.md", case.domain), authored_end));
        required.push((
            "references/capabilities-and-plugins.md".into(),
            authored_end,
        ));
    }
    if case.category == "graph" {
        required.push((
            "references/complexity-and-decomposition.md".into(),
            authored_end,
        ));
        required.push((
            "references/graph-patterns-and-dependencies.md".into(),
            authored_end,
        ));
    }
    if case.category == "followup" {
        required.push((
            "references/followups-cancellation-and-recovery.md".into(),
            preceding_model_results(
                &session.traces,
                session
                    .traces
                    .iter()
                    .position(|t| t.tool_name == "resolve_task_followup"),
            ),
        ));
    }
    let unread = required
        .iter()
        .filter(|(path, end)| !resource_read_before(&session.traces, path, *end))
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    check(
        &mut checks,
        "progressive_disclosure",
        unread.is_empty(),
        format!(
            "Missing complete reads before decision: {}",
            unread.join(", ")
        ),
    );
    check(
        &mut checks,
        "runtime_writer_serialization",
        safe_active_writers(&tasks),
        "Actual queued/running Task scopes do not overlap",
    );
    if case.category == "graph" {
        check(
            &mut checks,
            "one_durable_graph",
            graphs.len() == 1 && measured.len() >= 2,
            "Compound request stays in one real durable graph",
        );
        check(
            &mut checks,
            "graph_contract",
            measured.iter().all(|t| {
                ["graph_goal", "decomposition_rationale"]
                    .iter()
                    .all(|k| t.input[*k].as_str().is_some_and(|s| !s.trim().is_empty()))
            }),
            "Goal and decomposition rationale are persisted",
        );
        if case.expect.synthesis {
            check(&mut checks, "synthesis_reachability", graphs.len() == 1 && graph_has_synthesis(&graphs[0]), "One downstream Task receives every upstream outcome; meaningful synthesis requires review");
        }
    }
    if let Some(action) = &case.expect.followup_action {
        let expected_state = match action.as_str() {
            "cancel" => "cancelled",
            "clarify" => "held",
            _ => "released",
        };
        let resolved = session.traces.iter().any(|t| {
            t.tool_name == "resolve_task_followup"
                && success(t)
                && t.result.as_ref().is_some_and(|r| {
                    r["reviewed"] == true
                        && r["review_message_id"] == "eval-followup"
                        && r["tasks"].as_array().is_some_and(|items| {
                            session.seed_ids.iter().all(|id| {
                                items
                                    .iter()
                                    .any(|v| v["task_id"] == *id && v["state"] == expected_state)
                            })
                        })
                })
                && t.arguments["decisions"].as_array().is_some_and(|items| {
                    session.seed_ids.iter().all(|id| {
                        items
                            .iter()
                            .any(|v| v["task_id"] == *id && v["action"] == *action)
                    })
                })
        });
        check(
            &mut checks,
            "followup_decision",
            resolved,
            format!("Every old Task committed as {action}/{expected_state}"),
        );
        check(&mut checks, "followup_durable_state", tasks.iter().filter(|t| session.seed_ids.contains(&t.task_id)).all(|t|
            if action == "clarify" { t.input["followup_review_state"] == "held" && t.status != LocalTaskStatus::Cancelled }
            else { t.input.get("followup_review_state").is_none()
                && if action == "cancel" { t.status == LocalTaskStatus::Cancelled } else { t.status != LocalTaskStatus::Cancelled } }), "Held reviews stay visible; released/cancelled reviews are removed from Task input; cancellation status is durable");
    }
    if case.expect.supersedes {
        check(
            &mut checks,
            "replacement_link",
            !measured.is_empty()
                && measured.iter().all(|t| {
                    strings(&t.input["supersedes_task_ids"]).is_some_and(|ids| {
                        session.seed_ids.iter().all(|id| ids.contains(&id.as_str()))
                    })
                }),
            "Replacement Tasks retain supersedes lineage",
        );
    }
    Ok(CaseReport {
        id: case.id.clone(),
        category: case.category.clone(),
        structural_pass: checks.iter().all(|c| c.passed),
        semantic_review_required: true,
        semantic_status: "pending_human_review",
        rubric: case.rubric.clone(),
        checks,
        requests: session.requests,
        recovered_tool_errors,
        failure: session.failure.take(),
        final_text: session.final_text.take(),
        seed_task_ids: session.seed_ids,
        tasks,
        graphs,
        traces: session.traces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn read(offset: usize, length: usize, total: usize) -> Trace {
        Trace {
            step: 1,
            tool_name: "agent_skill_read_resource".into(),
            arguments: json!({}),
            status: LocalAgentToolStatus::Succeeded,
            error: None,
            result: Some(
                json!({"skill_ref": SKILL, "relative_path": "ref", "offset": offset,
                "returned_chars": length, "total_chars": total}),
            ),
        }
    }
    #[test]
    fn coverage_rejects_gaps_failures_and_future_reads() {
        assert!(!resource_read_before(&[read(0, 1, 20)], "ref", 1));
        assert!(!resource_read_before(
            &[read(0, 5, 20), read(6, 14, 20)],
            "ref",
            2
        ));
        assert!(resource_read_before(
            &[read(0, 10, 20), read(10, 10, 20)],
            "ref",
            2
        ));
        assert!(!resource_read_before(&[read(0, 20, 20)], "ref", 0));
        let mut failed = read(0, 20, 20);
        failed.status = LocalAgentToolStatus::Failed;
        assert!(!resource_read_before(&[failed], "ref", 1));
    }
    #[test]
    fn dataset_is_balanced() {
        let data: Dataset = serde_json::from_str(include_str!(
            "../../../evals/task-orchestration/scenarios.json"
        ))
        .unwrap();
        data.validate().unwrap();
    }
    #[test]
    fn same_step_read_cannot_inform_authored_contract() {
        let first = read(0, 20, 20);
        let mut decision = read(0, 20, 20);
        decision.tool_name = "create_task".into();
        let traces = vec![first, decision];
        assert_eq!(preceding_model_results(&traces, Some(1)), 0);
        assert!(!resource_read_before(
            &traces,
            "ref",
            preceding_model_results(&traces, Some(1))
        ));
        let mut traces = traces;
        traces[1].step = 2;
        assert!(resource_read_before(
            &traces,
            "ref",
            preceding_model_results(&traces, Some(1))
        ));
    }
    #[test]
    fn known_recovery_cannot_hide_unknown_or_same_step_results() {
        let mut failed = read(0, 0, 20);
        failed.tool_name = "create_task".into();
        failed.status = LocalAgentToolStatus::Failed;
        failed.result = Some(json!({"phase": "task_preparation", "side_effects_started": false}));
        let mut corrected = read(0, 0, 20);
        corrected.tool_name = "create_task".into();
        corrected.step = 2;
        let mut traces = vec![failed, corrected];
        assert!(recovered_error(&traces, 0));
        traces[1].step = 1;
        assert!(!recovered_error(&traces, 0));
        traces[1].step = 2;
        traces[0].status = LocalAgentToolStatus::NeedsReview;
        assert!(!recovered_error(&traces, 0));
        traces[0].status = LocalAgentToolStatus::Failed;
        traces[0].result = Some(json!({"phase": "task_preparation", "side_effects_started": true}));
        assert!(!recovered_error(&traces, 0));
    }
}
