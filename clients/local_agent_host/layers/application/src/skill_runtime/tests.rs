use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::*;
use serde_json::{json, Value};
use std::sync::Arc;

fn request(command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: uuid::Uuid::new_v4().to_string(),
        command,
    }
}

async fn check(
    runtime: &LocalAgentRuntime,
    arguments: Value,
) -> Result<(), LocalToolSkillGateDenial> {
    runtime
        .authorize_tool_skill_invocation("run-1", "capability_invoke", &arguments)
        .await
        .unwrap()
}

async fn claimed(runtime: &LocalAgentRuntime, name: &str, arguments: Value) -> LocalAgentToolClaim {
    let HostResult::Claim { claim: Some(claim) } = runtime
        .try_handle(request(HostCommand::ClaimNextRun(ClaimNextRunCommand {
            owner_user_id: "user-1".into(),
            worker_id: "model".into(),
            lease_duration_ms: 10_000,
        })))
        .await
        .expect("claim run")
    else {
        panic!("run claim")
    };
    runtime
        .try_handle(request(HostCommand::CommitStep(CommitStepCommand {
            owner_user_id: "user-1".into(),
            run_id: claim.run.run_id,
            claim_token: claim.claim_token,
            expected_version: claim.run.version,
            outcome: LocalAgentStepOutcome::WaitForTool {
                batch_id: uuid::Uuid::new_v4().to_string(),
                tool_calls: vec![LocalAgentToolCall {
                    call_id: uuid::Uuid::new_v4().to_string(),
                    tool_name: name.into(),
                    arguments,
                    side_effecting: false,
                    requires_approval: false,
                }],
                checkpoint: json!({}),
            },
        })))
        .await
        .expect("queue fact");
    let HostResult::ToolClaim { claim: Some(claim) } = runtime
        .try_handle(request(HostCommand::ClaimNextTool(ClaimNextToolCommand {
            owner_user_id: "user-1".into(),
            worker_id: "native".into(),
            lease_duration_ms: 10_000,
            include_tool_names: Some(vec![name.into()]),
            exclude_tool_names: vec![],
            execution_route: None,
        })))
        .await
        .expect("claim fact")
    else {
        panic!("tool claim")
    };
    claim
}

async fn fact(runtime: &LocalAgentRuntime, name: &str, arguments: Value, output: Value) {
    let claim = claimed(runtime, name, arguments).await;
    assert!(claim.preflight_outcome.is_none());
    runtime
        .try_handle(request(HostCommand::CommitTool(CommitToolCommand {
            owner_user_id: "user-1".into(),
            invocation_id: claim.invocation.invocation_id,
            claim_token: claim.claim_token,
            expected_version: claim.invocation.version,
            outcome: LocalAgentToolOutcome::Succeeded { output },
        })))
        .await
        .expect("commit fact");
}

#[tokio::test]
async fn discovered_plugin_gate_uses_durable_success_and_survives_restart() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::new(storage.clone());
    runtime.initialize("user-1").await.expect("initialize");
    let bundle = LocalSkillBundleSnapshot {
        schema_version: 2,
        bundle_revision: "bundle-2".into(),
        documents: vec![LocalSkillDocumentSnapshot {
            skill_ref: "product-skill:discovery".into(),
            name: "discovery".into(),
            role: "router".into(),
            description: "Discover".into(),
            instructions: "Describe before invocation".into(),
            instructions_sha256: "a".repeat(64),
            resources: vec![],
        }],
        tool_skill_bindings: vec![],
    };
    runtime
        .try_handle(request(HostCommand::PutCapabilityPolicySnapshot(
            PutCapabilityPolicySnapshotCommand {
                snapshot: LocalCapabilityPolicySnapshot {
                    owner_user_id: "user-1".into(),
                    profile_key: LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY.into(),
                    capability_policy_revision: "policy".into(),
                    instructions: Some(serde_json::to_string(&bundle).unwrap()),
                    prefixed_input_items: vec![],
                    tools: vec![],
                },
            },
        )))
        .await
        .expect("bundle");
    runtime
        .try_handle(request(HostCommand::CreateRun(CreateRunCommand {
            run_id: "run-1".into(),
            owner_user_id: "user-1".into(),
            owner_entity_type: "conversation".into(),
            owner_entity_id: "conversation".into(),
            profile_key: "task_execution".into(),
            model_config_ref: "model".into(),
            model_config_revision: "model-rev".into(),
            capability_policy_revision: "policy".into(),
            input: json!({}),
            max_iterations: 30,
        })))
        .await
        .expect("run");
    let invoke =
        json!({"plugin_option":"plugin_1", "tool_option":"tool_1", "arguments":{"format":"pdf"}});
    let denied = check(&runtime, invoke.clone()).await.unwrap_err();
    assert_eq!(
        denied.next_action.unwrap()["arguments"],
        json!({"plugin_option":"plugin_1", "tool_option":"tool_1"})
    );
    assert_eq!(
        check(&runtime, invoke.clone()).await.unwrap_err().error,
        "skill_binding_required"
    );
    let router = "plugin-skill:plugin_1:router";
    let pdf = "plugin-skill:plugin_1:pdf";
    let word = "plugin-skill:plugin_1:word";
    let description = json!({"binding_schema_version":1, "disclosure_version":2, "view":"tool", "provider_id":"plugin:document", "plugin_option":"plugin_1",
            "tools":[{"tool_option":"tool_1", "name":"document_create", "invocation_mode":"capability_invoke", "required_skill_refs":[router,pdf,word],
                "input_schema":{"type":"object"}, "schema_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "skill_gate":{"all_of":[router],"select_by_argument":{"pointer":"/format","map":{"pdf":pdf,"word":word}}}}]});
    let index = json!({"binding_schema_version":1,"disclosure_version":2,"view":"catalog",
            "provider_id":"plugin:document","plugin_option":"plugin_1","tools":[{"tool_option":"tool_1"}]});
    fact(
        &runtime,
        "capability_describe",
        json!({"plugin_option":"plugin_1"}),
        json!({"content":index,"is_error":false}),
    )
    .await;
    assert!(runtime
        .capability_disclosures_for_run("user-1", "run-1")
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        check(&runtime, invoke.clone()).await.unwrap_err().error,
        "skill_binding_required",
        "an index is not a callable tool definition"
    );
    let mut old_description = description.clone();
    old_description.as_object_mut().unwrap().remove("view");
    old_description
        .as_object_mut()
        .unwrap()
        .remove("disclosure_version");
    fact(
        &runtime,
        "capability_describe",
        json!({"plugin_option":"plugin_1"}),
        json!({"content":old_description,"is_error":false}),
    )
    .await;
    assert_eq!(
        check(&runtime, invoke.clone()).await.unwrap_err().error,
        "skill_binding_required",
        "old full-catalog descriptions must not authorize a tool"
    );
    fact(
        &runtime,
        "capability_describe",
        json!({"plugin_option":"plugin_1"}),
        json!({"content":description,"is_error":true}),
    )
    .await;
    assert_eq!(
        check(&runtime, invoke.clone()).await.unwrap_err().error,
        "skill_binding_required"
    );
    fact(
        &runtime,
        "capability_describe",
        json!({"plugin_option":"plugin_1","tool_option":"tool_1"}),
        json!({"content":description,"is_error":false}),
    )
    .await;
    assert_eq!(
        runtime
            .capability_disclosures_for_run("user-1", "run-1")
            .await
            .unwrap(),
        vec![LocalRunCapabilityDisclosure {
            plugin_option: "plugin_1".into(),
            tool_option: "tool_1".into(),
            tool_name: "document_create".into(),
            provider_id: "plugin:document".into(),
            schema_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
        }]
    );
    let mut catalog_description = description.clone();
    catalog_description["view"] = json!("catalog");
    catalog_description["tools"][0]["tool_option"] = json!("tool_2");
    catalog_description["tools"][0]["schema_sha256"] =
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    fact(
        &runtime,
        "capability_describe",
        json!({"plugin_option":"plugin_1"}),
        json!({"content":catalog_description,"is_error":false}),
    )
    .await;
    let batched = runtime
        .capability_disclosures_for_run("user-1", "run-1")
        .await
        .unwrap();
    assert_eq!(batched.len(), 2);
    assert_eq!(
        batched
            .iter()
            .map(|item| item.tool_option.as_str())
            .collect::<Vec<_>>(),
        vec!["tool_1", "tool_2"]
    );
    assert_ne!(
            check(
                &runtime,
                json!({"plugin_option":"plugin_1", "tool_option":"tool_2", "arguments":{"format":"pdf"}})
            )
            .await
            .unwrap_err()
            .error,
            "skill_binding_required",
            "a complete catalog entry must be a durable authorization fact"
        );
    assert_eq!(
        check(&runtime, invoke.clone())
            .await
            .unwrap_err()
            .required_skill_refs,
        vec![pdf, router]
    );
    fact(
        &runtime,
        "agent_skill_activate",
        json!({"skill_ref":router}),
        json!({"is_error":false,"content":{}}),
    )
    .await;
    let denied = check(&runtime, invoke.clone()).await.unwrap_err();
    assert_eq!(
        denied.next_action.unwrap()["arguments"],
        json!({"skill_refs":[pdf]})
    );
    fact(
        &runtime,
        "agent_skill_activate",
        json!({"skill_ref":pdf}),
        json!({"is_error":true,"content":"rejected"}),
    )
    .await;
    assert_eq!(
        check(&runtime, invoke.clone())
            .await
            .unwrap_err()
            .required_skill_refs,
        vec![pdf]
    );
    fact(
        &runtime,
        "agent_skill_activate",
        json!({"skill_ref":pdf}),
        json!({"is_error":false,"content":{}}),
    )
    .await;
    let restarted = LocalAgentRuntime::new(storage);
    assert!(check(&restarted, invoke.clone()).await.is_ok());
    let mut second = description.clone();
    second["disclosure_version"] = json!(2);
    second["view"] = json!("tool");
    second["tools"][0]["tool_option"] = json!("tool_2");
    fact(
        &restarted,
        "capability_describe",
        json!({"plugin_option":"plugin_1","tool_option":"tool_2"}),
        json!({"content":second,"is_error":false}),
    )
    .await;
    fact(
        &restarted,
        "capability_describe",
        json!({"plugin_option":"plugin_1","offset":8}),
        json!({"content":index,"is_error":false}),
    )
    .await;
    assert!(
        check(&restarted, invoke.clone()).await.is_ok(),
        "describing another tool or index page must retain the first tool's durable gate"
    );
    assert_eq!(
        check(
            &restarted,
            json!({"plugin_option":"plugin_1","tool_option":"tool_1","arguments":{"format":"word"}})
        )
        .await
        .unwrap_err()
        .required_skill_refs,
        vec![word]
    );
    fact(
        &restarted,
        "agent_skill_activate",
        json!({"skill_refs":[word,router]}),
        json!({"is_error":false,"content":{"skills":[]}}),
    )
    .await;
    assert_eq!(
        restarted
            .activated_skill_refs_for_run("user-1", "run-1")
            .await
            .unwrap(),
        vec![pdf.to_string(), router.to_string(), word.to_string()]
    );
    assert!(check(
        &restarted,
        json!({"plugin_option":"plugin_1","tool_option":"tool_1","arguments":{"format":"word"}})
    )
    .await
    .is_ok());
    for arguments in [json!({"format":"unknown"}), json!({}), json!({"format":1})] {
        assert_eq!(
            check(
                &restarted,
                json!({"plugin_option":"plugin_1","tool_option":"tool_1","arguments":arguments})
            )
            .await
            .unwrap_err()
            .error,
            "skill_binding_invalid"
        );
    }
    assert_eq!(
        check(
            &restarted,
            json!({"plugin_option":"plugin_2","tool_option":"tool_1","arguments":{}})
        )
        .await
        .unwrap_err()
        .error,
        "skill_binding_required"
    );
    let mut direct = description.clone();
    direct["tools"][0]["invocation_mode"] = json!("direct_task_tool");
    fact(
        &restarted,
        "capability_describe",
        json!({"plugin_option":"plugin_1"}),
        json!({"content":direct,"is_error":false}),
    )
    .await;
    assert_eq!(
        check(&restarted, invoke).await.unwrap_err().error,
        "skill_binding_invalid"
    );
    assert_eq!(
        restarted
            .authorize_tool_skill_for_run("run-1", "external_mcp__missing__read")
            .await
            .unwrap()
            .unwrap_err()
            .error,
        "skill_binding_required"
    );
    let claim = claimed(
        &restarted,
        "capability_invoke",
        json!({"plugin_option":"plugin_2","tool_option":"tool_1","arguments":{}}),
    )
    .await;
    let Some(LocalAgentToolOutcome::Failed { error, .. }) = claim.preflight_outcome else {
        panic!("IPC claim must carry the shared dynamic gate denial");
    };
    assert_eq!(error, "skill_binding_required");
}
