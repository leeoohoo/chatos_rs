use super::{
    checks::{self, Capabilities, CaseReport, Dataset, Scenario},
    runtime::*,
};
use chatos_ai_runtime::ModelRuntimeConfig;
use chatos_local_agent_protocol::LocalCapabilityPolicySnapshot;
use serde::Serialize;
use serde_json::{json, Value};

fn call(name: &str, arguments: Value) -> ReplayStep {
    ReplayStep {
        calls: vec![ReplayCall {
            name: name.into(),
            arguments,
        }],
        final_text: None,
    }
}
fn final_step(text: &str) -> ReplayStep {
    ReplayStep {
        calls: vec![],
        final_text: Some(text.into()),
    }
}
fn activate() -> ReplayStep {
    call("agent_skill_activate", json!({"skill_ref": SKILL}))
}
fn read(path: &str) -> ReplayStep {
    call(
        "agent_skill_read_resource",
        json!({"skill_ref": SKILL, "relative_path": path, "max_chars": 50000}),
    )
}
fn task(title: &str, objective: &str, output: &str, allowed: &Capabilities) -> Value {
    json!({"title": title, "objective": objective, "expected_outputs": [output],
        "acceptance_criteria": [format!("{output}以实际项目证据核验，保留请求的范围与原始资料")],
        "requires_execution": allowed.builtins.iter().any(|s| s == "CodeMaintainerWrite"),
        "enabled_builtin_kinds": allowed.builtins,
        "plugin_hints": allowed.plugins.iter().map(|key| json!({"plugin_key": key})).collect::<Vec<_>>()})
}

fn steps(case: &Scenario, allowed: &Capabilities, seed_id: Option<&str>) -> Vec<ReplayStep> {
    if case.category == "answer" {
        return vec![final_step("401 是未认证，403 是已识别请求者但没有权限。")];
    }
    let mut out = vec![activate()];
    out.push(read(&format!("references/{}.md", case.domain)));
    out.push(read("references/capabilities-and-plugins.md"));
    if let Some(seed) = seed_id {
        out.push(read("references/followups-cancellation-and-recovery.md"));
        let action = case.expect.followup_action.as_deref().unwrap();
        out.push(call("resolve_task_followup", json!({"review_message_id": "eval-followup",
            "decisions": [{"task_id": seed, "action": action, "reason": "按最新请求处理已有任务"}]})));
        if case.expect.supersedes {
            let mut replacement = task(
                "受限的替代工作",
                &case.prompt,
                "docs/facts.md：内部事实说明",
                allowed,
            );
            replacement["supersedes_task_ids"] = json!([seed]);
            out.push(call("create_task", replacement));
        }
    } else if case.category == "single" {
        out.push(call(
            "create_task",
            task(
                "配置加载入口检查",
                &case.prompt,
                "配置路径决定位置与源码依据报告",
                allowed,
            ),
        ));
    } else {
        out.push(read("references/complexity-and-decomposition.md"));
        out.push(read("references/graph-patterns-and-dependencies.md"));
        let (first, second) = (
            (
                "搜索模块空输入提示",
                "src/search 空输入改动和说明",
                "src/search",
            ),
            (
                "导出模块空输入提示",
                "src/export 空输入改动和说明",
                "src/export",
            ),
        );
        let mut a = task(
            first.0,
            &format!("{}；只负责 {}", case.prompt, first.0),
            first.1,
            allowed,
        );
        a["client_ref"] = json!("first");
        let mut b = task(
            second.0,
            &format!("{}；只负责 {}", case.prompt, second.0),
            second.1,
            allowed,
        );
        b["client_ref"] = json!("second");
        a["owned_paths"] = json!([first.2]);
        b["owned_paths"] = json!([second.2]);
        // Use a read-only consolidation node, independent of writer ownership.
        let mut sink = task(
            "统一检查",
            "依据两个上游实际交付逐条核验需求与一致性，不再修改文件",
            "统一交付检查报告",
            &Capabilities {
                builtins: vec!["CodeMaintainerRead".into()],
                plugins: vec![],
                external: vec![],
            },
        );
        sink["client_ref"] = json!("check");
        sink["prerequisite_refs"] = json!(["first", "second"]);
        let mut tasks = vec![a, b, sink];
        if case.id == "graph-campaign" {
            tasks.clear();
            let mut brief = task(
                "统一受众与口径",
                "阅读 product-brief.md，确定可引用事实、核心卖点与各渠道受众，不发布",
                "共享受众与事实口径报告",
                &Capabilities {
                    builtins: vec!["CodeMaintainerRead".into()],
                    plugins: vec![],
                    external: vec![],
                },
            );
            brief["client_ref"] = json!("brief");
            tasks.push(brief);
            for (reference, title, path, objective) in [
                (
                    "web",
                    "官网长页",
                    "docs/landing.md",
                    "根据共享事实口径写官网长页并保存",
                ),
                (
                    "mail",
                    "销售邮件",
                    "docs/emails.md",
                    "根据共享事实口径写三封销售邮件并保存，不发送",
                ),
                (
                    "social",
                    "社交短文",
                    "docs/social.md",
                    "根据共享事实口径写五条社交短文并保存，不发布",
                ),
            ] {
                let mut item = task(title, objective, path, allowed);
                item["client_ref"] = json!(reference);
                item["owned_paths"] = json!([path]);
                item["prerequisite_refs"] = json!(["brief"]);
                tasks.push(item);
            }
            let mut sink = task(
                "跨渠道一致性检查",
                "等待官网、邮件、社交交付，核对事实与口径、条数及范围，不发布",
                "首发内容统一验收报告",
                &Capabilities {
                    builtins: vec!["CodeMaintainerRead".into()],
                    plugins: vec![],
                    external: vec![],
                },
            );
            sink["client_ref"] = json!("check");
            sink["prerequisite_refs"] = json!(["web", "mail", "social"]);
            tasks.push(sink);
        }
        out.push(call("create_tasks_with_prerequisites", json!({
            "graph_goal": case.prompt, "decomposition_rationale": "不同文件产物分开负责，可安全并行，最终等待全部产物统一验收", "tasks": tasks})));
    }
    if case.bounds().2 != 0 {
        out.push(call("wait_for_task_completion", json!({})));
    }
    out.push(final_step(
        if case.expect.followup_action.as_deref() == Some("clarify") {
            "已暂停旧任务。请确认是继续原计划，还是改为新的交付？"
        } else if case.expect.followup_action.as_deref() == Some("cancel")
            && !case.expect.supersedes
        {
            "已取消旧任务的后续执行，保留已有文件。"
        } else {
            "已安排并交接后台工作；执行结果将回到当前对话。"
        },
    ));
    out
}

pub async fn drive(
    session: &mut Session,
    profile: &dyn chatos_local_agent_runtime::LocalAgentProfile,
    budget: usize,
) {
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        for _ in 0..budget {
            if session.step(profile).await? {
                return Ok(());
            }
        }
        Err::<(), String>("planning_request_budget_exceeded".into())
    })
    .await;
    session.failure = match result {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(_) => Some("planning_case_timeout".into()),
    };
}

#[derive(Serialize)]
pub struct NegativeControl {
    pub id: &'static str,
    pub passed: bool,
    pub failure: Option<String>,
    pub traces: Vec<Trace>,
}
pub async fn run(
    data: &Dataset,
    snapshots: &[LocalCapabilityPolicySnapshot],
    config: &ModelRuntimeConfig,
) -> Result<(Vec<CaseReport>, Vec<NegativeControl>), String> {
    let mut reports = Vec::new();
    for id in [
        "answer-dev",
        "single-inspection",
        "graph-parallel-code",
        "graph-campaign",
        "followup-status",
        "followup-stop",
        "followup-clarify",
        "followup-no-publish",
    ] {
        let case = data
            .scenarios
            .iter()
            .find(|s| s.id == id)
            .ok_or("missing_smoke_case")?;
        let initial = if case.seed.is_some() {
            "请执行下面的原始工作"
        } else {
            &case.prompt
        };
        let mut session = Session::new(snapshots, config.clone(), initial).await?;
        if let Some(seed) = &case.seed {
            session.seed_followup(seed, &case.prompt).await?;
        }
        let profile = replay_profile(
            session.control.clone(),
            steps(
                case,
                &data.capability_profiles[&case.capabilities],
                session.seed_ids.first().map(String::as_str),
            ),
        )?;
        drive(&mut session, profile.as_ref(), 16).await;
        reports.push(
            checks::report(case, &data.capability_profiles[&case.capabilities], session).await?,
        );
    }
    let simple = task(
        "只读检查",
        "核实配置",
        "配置报告",
        &data.capability_profiles["read"],
    );
    let mut left = task(
        "修改一",
        "修改 src",
        "src 修改",
        &data.capability_profiles["write"],
    );
    let mut right = task(
        "修改二",
        "修改 src/config",
        "config 修改",
        &data.capability_profiles["write"],
    );
    left["client_ref"] = json!("a");
    left["owned_paths"] = json!(["src"]);
    right["client_ref"] = json!("b");
    right["owned_paths"] = json!(["src/config"]);
    let invalid_graph = json!({"graph_goal": "交叠写入", "decomposition_rationale": "控制样例",
        "tasks": [left, right]});
    let mut repaired_graph = invalid_graph.clone();
    repaired_graph["tasks"][1]["owned_paths"] = json!(["src-other"]);
    let plans = [
        (
            "missing-activation",
            vec![call("create_task", simple.clone()), final_step("done")],
            false,
        ),
        (
            "overlapping-writers",
            vec![
                activate(),
                call("create_tasks_with_prerequisites", invalid_graph.clone()),
                final_step("done"),
            ],
            false,
        ),
        (
            "business-tool-unavailable",
            vec![call(
                "terminal_exec",
                json!({"command": "must never execute"}),
            )],
            true,
        ),
        (
            "post-handoff-tool-unavailable",
            vec![
                activate(),
                call("create_task", simple.clone()),
                call("wait_for_task_completion", json!({})),
                call("list_tasks", json!({})),
            ],
            true,
        ),
        (
            "repeated-handoff-unavailable",
            vec![
                activate(),
                call("create_task", simple),
                call("wait_for_task_completion", json!({})),
                call("wait_for_task_completion", json!({})),
            ],
            true,
        ),
    ];
    let mut negatives = Vec::new();
    for (id, steps, unavailable) in plans {
        let mut session = Session::new(snapshots, config.clone(), "Negative control").await?;
        let profile = replay_profile(session.control.clone(), steps)?;
        drive(&mut session, profile.as_ref(), 16).await;
        let tasks = session.tasks().await?;
        let passed = if unavailable {
            session.failure.as_deref() == Some("replay_called_unavailable_tool")
        } else {
            tasks.is_empty()
                && session.traces.iter().any(|trace| {
                    trace.status == chatos_local_agent_protocol::LocalAgentToolStatus::Failed
                })
        };
        negatives.push(NegativeControl {
            id,
            passed,
            failure: session.failure,
            traces: session.traces,
        });
    }
    let mut recovery = Session::new(snapshots, config.clone(), "Repair a rejected graph").await?;
    let profile = replay_profile(
        recovery.control.clone(),
        vec![
            activate(),
            call("create_tasks_with_prerequisites", invalid_graph),
            call("create_tasks_with_prerequisites", repaired_graph.clone()),
            call("wait_for_task_completion", json!({})),
            final_step("修正路径后已安排工作"),
        ],
    )?;
    drive(&mut recovery, profile.as_ref(), 16).await;
    let passed = recovery.failure.is_none()
        && recovery.tasks().await?.len() == 2
        && recovery
            .traces
            .iter()
            .filter(|t| t.tool_name == "create_tasks_with_prerequisites")
            .map(|t| t.status)
            .collect::<Vec<_>>()
            == [
                chatos_local_agent_protocol::LocalAgentToolStatus::Failed,
                chatos_local_agent_protocol::LocalAgentToolStatus::Succeeded,
            ];
    negatives.push(NegativeControl {
        id: "graph-rejection-recovery",
        passed,
        failure: recovery.failure,
        traces: recovery.traces,
    });
    // Model-argument errors must return known failures and allow correction through the
    // real ledger. These are safety/recovery controls, not semantic model-quality evidence.
    let mut missing_contract = task(
        "Read",
        "Read configuration",
        "Configuration report",
        &data.capability_profiles["read"],
    );
    missing_contract
        .as_object_mut()
        .unwrap()
        .remove("acceptance_criteria");
    let corrected_single = task(
        "Read",
        "Read configuration",
        "Configuration report",
        &data.capability_profiles["read"],
    );
    let mut bad_dependency = repaired_graph.clone();
    bad_dependency["tasks"][1]["prerequisite_refs"] = json!(["missing"]);
    for (id, name, bad, good, expected_count) in [
        (
            "single-contract-recovery",
            "create_task",
            missing_contract,
            corrected_single,
            1,
        ),
        (
            "graph-dependency-recovery",
            "create_tasks_with_prerequisites",
            bad_dependency,
            repaired_graph,
            2,
        ),
    ] {
        let mut session =
            Session::new(snapshots, config.clone(), "Repair invalid model arguments").await?;
        let profile = replay_profile(
            session.control.clone(),
            vec![
                activate(),
                call(name, bad),
                call(name, good),
                call("wait_for_task_completion", json!({})),
                final_step("已修正并交接"),
            ],
        )?;
        drive(&mut session, profile.as_ref(), 16).await;
        let passed = session.failure.is_none()
            && session.tasks().await?.len() == expected_count
            && session
                .traces
                .iter()
                .filter(|t| t.tool_name == name)
                .map(|t| t.status)
                .collect::<Vec<_>>()
                == [
                    chatos_local_agent_protocol::LocalAgentToolStatus::Failed,
                    chatos_local_agent_protocol::LocalAgentToolStatus::Succeeded,
                ];
        negatives.push(NegativeControl {
            id,
            passed,
            failure: session.failure,
            traces: session.traces,
        });
    }
    let case = data
        .scenarios
        .iter()
        .find(|case| case.id == "followup-status")
        .ok_or("missing_followup_case")?;
    let mut session = Session::new(snapshots, config.clone(), "Original inspection").await?;
    session
        .seed_followup(case.seed.as_ref().ok_or("missing_seed")?, &case.prompt)
        .await?;
    let seed = &session.seed_ids[0];
    let decision =
        json!({"task_id": seed, "action": "continue", "reason": "仅查询进度，保留旧工作"});
    let correct = json!({"review_message_id": "eval-followup", "decisions": [decision]});
    let invalid = [
        json!({"review_message_id": "stale", "decisions": [decision]}),
        json!({"review_message_id": "eval-followup", "decisions": []}),
        json!({"review_message_id": "eval-followup", "decisions": [decision, decision]}),
        json!({"review_message_id": "eval-followup", "decisions": [{"task_id": "foreign", "action": "continue", "reason": "wrong id"}]}),
    ];
    let mut pending_preserved = true;
    for args in invalid {
        let profile = replay_profile(
            session.control.clone(),
            vec![call("resolve_task_followup", args)],
        )?;
        if session.step(profile.as_ref()).await? {
            return Err("premature_review_completion".into());
        }
        pending_preserved &= session.tasks().await?.iter().all(|task| {
            task.input["followup_review_state"] == "pending"
                && task.status != chatos_local_agent_protocol::LocalTaskStatus::Cancelled
        });
    }
    let profile = replay_profile(
        session.control.clone(),
        vec![
            call("resolve_task_followup", correct),
            call("wait_for_task_completion", json!({})),
            final_step("旧工作继续"),
        ],
    )?;
    drive(&mut session, profile.as_ref(), 16).await;
    let statuses = session
        .traces
        .iter()
        .filter(|t| t.tool_name == "resolve_task_followup")
        .map(|t| t.status)
        .collect::<Vec<_>>();
    let passed = pending_preserved
        && session.failure.is_none()
        && statuses
            == [
                chatos_local_agent_protocol::LocalAgentToolStatus::Failed,
                chatos_local_agent_protocol::LocalAgentToolStatus::Failed,
                chatos_local_agent_protocol::LocalAgentToolStatus::Failed,
                chatos_local_agent_protocol::LocalAgentToolStatus::Failed,
                chatos_local_agent_protocol::LocalAgentToolStatus::Succeeded,
            ];
    negatives.push(NegativeControl {
        id: "followup-review-argument-recovery",
        passed,
        failure: session.failure,
        traces: session.traces,
    });
    // Test checker sensitivity using a real stored Task, not a handcrafted record.
    if let Some(task) = reports.iter().flat_map(|r| &r.tasks).next() {
        let mut broad = task.clone();
        broad.input["tool_options"]["enabled_builtin_kinds"] = json!(["TerminalController"]);
        negatives.push(NegativeControl {
            id: "checker-overbroad-capability",
            passed: !checks::capabilities_valid(&[broad], &data.capability_profiles["read"]),
            failure: None,
            traces: vec![],
        });
        negatives.push(NegativeControl {
            id: "checker-duplicate-outputs",
            passed: !checks::contracts_valid(&[task.clone(), task.clone()]),
            failure: None,
            traces: vec![],
        });
    }
    Ok((reports, negatives))
}
