// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LocalSkillDocumentSnapshot,
    LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME, LOCAL_AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub const AGENT_SKILL_TOOL_NAMES: [&str; 2] = [
    LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
    LOCAL_AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
];

pub const AGENT_SKILL_READ_ONLY_TOOLS: [&str; 2] = AGENT_SKILL_TOOL_NAMES;
pub const AGENT_SKILL_APPROVAL_EXEMPT_TOOLS: [&str; 2] = AGENT_SKILL_TOOL_NAMES;

pub fn skill_model_tools() -> Vec<Value> {
    vec![
        model_tool(
            LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME,
            "Activate one advertised local Skill for this Run and load its core instructions. Use the returned resource manifest to read only the supporting references needed for the current request.",
            json!({
                "type": "object",
                "properties": {
                    "skill_ref": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 512,
                        "description": "Exact Skill reference advertised in the current Main Chat Skill catalog."
                    }
                },
                "required": ["skill_ref"],
                "additionalProperties": false
            }),
        ),
        model_tool(
            LOCAL_AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
            "Read a page of one supporting resource from an already activated local Skill. Read only resources relevant to the current request.",
            json!({
                "type": "object",
                "properties": {
                    "skill_ref": {"type": "string", "minLength": 1, "maxLength": 512},
                    "relative_path": {"type": "string", "minLength": 1, "maxLength": 1024},
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "default": 0,
                        "description": "Zero-based Unicode character offset."
                    },
                    "max_chars": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 160000,
                        "default": 160000
                    }
                },
                "required": ["skill_ref", "relative_path"],
                "additionalProperties": false
            }),
        ),
    ]
}

#[derive(Clone)]
pub struct LocalSkillToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
}

impl LocalSkillToolExecutor {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Local Skill tool owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
        })
    }

    pub fn register_into(&self, registry: &mut LocalToolRegistry) -> Result<(), String> {
        let executor: Arc<dyn LocalToolExecutor> = Arc::new(self.clone());
        for name in AGENT_SKILL_TOOL_NAMES {
            registry.register_shared(name, Arc::clone(&executor))?;
        }
        Ok(())
    }

    async fn execute(&self, invocation: &LocalAgentToolInvocationRecord) -> Result<Value, String> {
        let (run, bundle) = self
            .runtime
            .skill_bundle_for_run(&invocation.run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "No local Skill bundle is available for this Run".to_string())?;
        if run.owner_user_id != self.owner_user_id {
            return Err("Local Skill tool Run does not belong to the active owner".to_string());
        }

        match invocation.tool_name.as_str() {
            LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME => {
                let args: SkillRefArgs = decode(invocation)?;
                let document = bundle.document(&args.skill_ref).ok_or_else(|| {
                    format!(
                        "Skill is not present in this Run bundle: {}",
                        args.skill_ref
                    )
                })?;
                Ok(json!({
                    "activated": true,
                    "skill_ref": document.skill_ref,
                    "name": document.name,
                    "role": document.role,
                    "description": document.description,
                    "instructions": document.instructions,
                    "instructions_sha256": document.instructions_sha256,
                    "resources": resource_manifest(document),
                    "bundle_revision": bundle.bundle_revision
                }))
            }
            LOCAL_AGENT_SKILL_READ_RESOURCE_TOOL_NAME => {
                let args: ReadResourceArgs = decode(invocation)?;
                let document =
                    activated_document(&self.runtime, &invocation.run_id, &bundle, &args.skill_ref)
                        .await?;
                let resource = document
                    .resources
                    .iter()
                    .find(|resource| resource.relative_path == args.relative_path)
                    .ok_or_else(|| {
                        format!(
                            "Skill resource is not present in the manifest: {}:{}",
                            args.skill_ref, args.relative_path
                        )
                    })?;
                let offset = args.offset.unwrap_or(0);
                let max_chars = args.max_chars.unwrap_or(160_000);
                if max_chars == 0 || max_chars > 160_000 {
                    return Err("max_chars must be between 1 and 160000".to_string());
                }
                let total_chars = resource.content.chars().count();
                if offset > total_chars {
                    return Err(format!(
                        "offset exceeds resource length: {offset} > {total_chars}"
                    ));
                }
                let content = resource
                    .content
                    .chars()
                    .skip(offset)
                    .take(max_chars)
                    .collect::<String>();
                let returned_chars = content.chars().count();
                let next_offset = offset + returned_chars;
                let truncated = next_offset < total_chars;
                Ok(json!({
                    "skill_ref": document.skill_ref,
                    "relative_path": resource.relative_path,
                    "content": content,
                    "sha256": resource.sha256,
                    "offset": offset,
                    "returned_chars": returned_chars,
                    "total_chars": total_chars,
                    "next_offset": truncated.then_some(next_offset),
                    "truncated": truncated,
                    "bundle_revision": bundle.bundle_revision
                }))
            }
            name => Err(format!("unsupported local Skill tool: {name}")),
        }
    }
}

#[async_trait]
impl LocalToolExecutor for LocalSkillToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        Ok(match self.execute(invocation).await {
            Ok(output) => LocalAgentToolOutcome::Succeeded { output },
            Err(error) => LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": "local_skill_tool"}),
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillRefArgs {
    skill_ref: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadResourceArgs {
    skill_ref: String,
    relative_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    max_chars: Option<usize>,
}

fn decode<T: for<'de> Deserialize<'de>>(
    invocation: &LocalAgentToolInvocationRecord,
) -> Result<T, String> {
    serde_json::from_value(invocation.arguments.clone())
        .map_err(|error| format!("invalid {} input: {error}", invocation.tool_name))
}

async fn activated_document<'a>(
    runtime: &LocalAgentRuntime,
    run_id: &str,
    bundle: &'a chatos_local_agent_protocol::LocalSkillBundleSnapshot,
    skill_ref: &str,
) -> Result<&'a LocalSkillDocumentSnapshot, String> {
    let document = bundle
        .document(skill_ref)
        .ok_or_else(|| format!("Skill is not present in this Run bundle: {skill_ref}"))?;
    if !runtime
        .skill_is_activated(run_id, skill_ref)
        .await
        .map_err(|error| error.to_string())?
    {
        return Err(format!(
            "Skill must be activated before reading resources: {skill_ref}"
        ));
    }
    Ok(document)
}

fn resource_manifest(document: &LocalSkillDocumentSnapshot) -> Vec<Value> {
    document
        .resources
        .iter()
        .map(|resource| {
            json!({
                "relative_path": resource.relative_path,
                "sha256": resource.sha256,
                "character_count": resource.content.chars().count()
            })
        })
        .collect()
}

fn model_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": parameters
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalToolScheduler, ToolSchedulerTick};
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CreateRunCommand,
        GetRunCommand, HostCommand, HostRequestEnvelope, HostResult, LocalAgentStepOutcome,
        LocalAgentToolCall, LocalAgentToolOutcome, LocalCapabilityPolicySnapshot,
        LocalSkillBundleSnapshot, LocalSkillDocumentSnapshot, LocalSkillResourceSnapshot,
        LocalToolSkillActivationPolicy, LocalToolSkillBindingSnapshot,
        PutCapabilityPolicySnapshotCommand, LOCAL_AGENT_PROTOCOL_VERSION,
        LOCAL_SKILL_BUNDLE_PROFILE_KEY, LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY,
    };

    fn envelope(prefix: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: format!("{prefix}-{}", uuid::Uuid::new_v4()),
            command,
        }
    }

    fn digest(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    fn bundle() -> LocalSkillBundleSnapshot {
        LocalSkillBundleSnapshot {
            schema_version: 1,
            bundle_revision: "bundle-1".into(),
            documents: vec![LocalSkillDocumentSnapshot {
                skill_ref: "product-skill:task".into(),
                name: "task".into(),
                role: "specialist".into(),
                description: "Task orchestration".into(),
                instructions: "Decompose compound requests.".into(),
                instructions_sha256: digest('a'),
                resources: vec![LocalSkillResourceSnapshot {
                    relative_path: "references/example.md".into(),
                    content: "甲乙丙丁".into(),
                    sha256: digest('b'),
                }],
            }],
            tool_skill_bindings: vec![LocalToolSkillBindingSnapshot {
                provider_id: "chatos.local.main-chat-task".into(),
                binding_id: "main-chat.task-orchestration".into(),
                tool_names: vec!["create_task".into()],
                router_skill_ref: "product-skill:task".into(),
                specialist_skill_ref: "product-skill:task".into(),
                activation_policy: LocalToolSkillActivationPolicy::OnDemand,
            }],
        }
    }

    fn task_bundle() -> LocalSkillBundleSnapshot {
        let mut bundle = bundle();
        bundle.bundle_revision = "task-bundle-1".into();
        bundle.tool_skill_bindings[0] = LocalToolSkillBindingSnapshot {
            provider_id: "chatos.builtin.project-read".into(),
            binding_id: "project-files.read".into(),
            tool_names: vec!["read_file".into()],
            router_skill_ref: "product-skill:task".into(),
            specialist_skill_ref: "product-skill:task".into(),
            activation_policy: LocalToolSkillActivationPolicy::OnDemand,
        };
        bundle
    }

    #[test]
    fn resource_reader_uses_the_unified_model_page_budget() {
        let tools = skill_model_tools();
        let max_chars = &tools[1]["parameters"]["properties"]["max_chars"];
        assert_eq!(max_chars["maximum"], 160_000);
        assert_eq!(max_chars["default"], 160_000);
    }

    #[tokio::test]
    async fn task_execution_uses_its_profile_specific_skill_bundle() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::new(storage);
        runtime.initialize("user-1").await.expect("initialize");
        for (profile_key, snapshot) in [
            (LOCAL_SKILL_BUNDLE_PROFILE_KEY, bundle()),
            (LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY, task_bundle()),
        ] {
            runtime
                .try_handle(envelope(
                    profile_key,
                    HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                        snapshot: LocalCapabilityPolicySnapshot {
                            owner_user_id: "user-1".into(),
                            profile_key: profile_key.into(),
                            capability_policy_revision: "policy-1".into(),
                            instructions: Some(
                                serde_json::to_string(&snapshot).expect("bundle json"),
                            ),
                            prefixed_input_items: vec![],
                            tools: vec![],
                        },
                    }),
                ))
                .await
                .expect("publish bundle");
        }
        for (run_id, profile_key) in [
            ("run-task-profile", "task_execution"),
            ("run-main-profile", "main_chat"),
        ] {
            runtime
                .try_handle(envelope(
                    run_id,
                    HostCommand::CreateRun(CreateRunCommand {
                        run_id: run_id.into(),
                        owner_user_id: "user-1".into(),
                        owner_entity_type: "conversation".into(),
                        owner_entity_id: "conversation-1".into(),
                        profile_key: profile_key.into(),
                        model_config_ref: "model-1".into(),
                        model_config_revision: "revision-1".into(),
                        capability_policy_revision: "policy-1".into(),
                        input: json!({"message": "read"}),
                        max_iterations: 4,
                    }),
                ))
                .await
                .expect("create run");
        }

        let denied = runtime
            .authorize_tool_skill_for_run("run-task-profile", "read_file")
            .await
            .expect("task gate")
            .expect_err("Task Execution must use its own bundle");
        assert_eq!(denied.binding_id, "project-files.read");
        assert_eq!(denied.required_skill_refs, ["product-skill:task"]);
        let response = runtime
            .try_handle(envelope(
                "get-task-bundle",
                HostCommand::GetRunSkillBundle(GetRunCommand {
                    owner_user_id: "user-1".into(),
                    run_id: "run-task-profile".into(),
                }),
            ))
            .await
            .expect("fetch frozen Task bundle");
        assert_eq!(
            response,
            HostResult::RunSkillBundle {
                run_id: "run-task-profile".into(),
                bundle: Some(task_bundle()),
            }
        );
        assert!(runtime
            .try_handle(envelope(
                "get-foreign-bundle",
                HostCommand::GetRunSkillBundle(GetRunCommand {
                    owner_user_id: "another-user".into(),
                    run_id: "run-task-profile".into(),
                }),
            ))
            .await
            .is_err());
        assert_eq!(
            runtime
                .authorize_tool_skill_for_run("run-main-profile", "read_file")
                .await
                .expect("main gate"),
            Ok(())
        );
    }

    #[tokio::test]
    async fn external_tool_claim_carries_shared_skill_gate_denial() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::new(storage);
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                "external-gate-bundle",
                HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                    snapshot: LocalCapabilityPolicySnapshot {
                        owner_user_id: "user-1".into(),
                        profile_key: LOCAL_SKILL_BUNDLE_PROFILE_KEY.into(),
                        capability_policy_revision: "policy-1".into(),
                        instructions: Some(serde_json::to_string(&bundle()).expect("bundle json")),
                        prefixed_input_items: vec![],
                        tools: vec![],
                    },
                }),
            ))
            .await
            .expect("publish bundle");
        runtime
            .try_handle(envelope(
                "external-gate-create",
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-external-gate".into(),
                    owner_user_id: "user-1".into(),
                    owner_entity_type: "conversation".into(),
                    owner_entity_id: "conversation-1".into(),
                    profile_key: "main_chat".into(),
                    model_config_ref: "model-1".into(),
                    model_config_revision: "revision-1".into(),
                    capability_policy_revision: "policy-1".into(),
                    input: json!({"message": "create a task"}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create run");
        let claimed = runtime
            .try_handle(envelope(
                "external-gate-claim-run",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "model-worker".into(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .expect("claim run");
        let HostResult::Claim { claim: Some(claim) } = claimed else {
            panic!("expected run claim")
        };
        runtime
            .try_handle(envelope(
                "external-gate-wait",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".into(),
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: "external-gate-batch".into(),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: "external-gate-call".into(),
                            tool_name: "create_task".into(),
                            arguments: json!({"title": "Task"}),
                            side_effecting: true,
                            requires_approval: false,
                        }],
                        checkpoint: json!({"response_id": "response-1"}),
                    },
                }),
            ))
            .await
            .expect("queue gated tool");
        let claimed_tool = runtime
            .try_handle(envelope(
                "external-gate-claim-tool",
                HostCommand::ClaimNextTool(ClaimNextToolCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "native-worker".into(),
                    lease_duration_ms: 10_000,
                    include_tool_names: Some(vec!["create_task".into()]),
                    exclude_tool_names: vec![],
                    execution_route: None,
                }),
            ))
            .await
            .expect("claim gated tool");
        let HostResult::ToolClaim {
            claim: Some(tool_claim),
        } = claimed_tool
        else {
            panic!("expected tool claim")
        };
        let Some(LocalAgentToolOutcome::Failed { error, detail }) = tool_claim.preflight_outcome
        else {
            panic!("expected shared preflight denial")
        };
        assert_eq!(error, "skill_activation_required");
        assert_eq!(detail["tool_name"], "create_task");
        assert_eq!(detail["binding_id"], "main-chat.task-orchestration");
        assert_eq!(detail["required_skill_refs"], json!(["product-skill:task"]));
    }

    #[tokio::test]
    async fn activation_is_durable_and_resources_are_character_paged() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage.clone()));
        runtime.initialize("user-1").await.expect("initialize");
        let encoded = serde_json::to_string(&bundle()).expect("bundle json");
        runtime
            .try_handle(envelope(
                "bundle",
                HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                    snapshot: LocalCapabilityPolicySnapshot {
                        owner_user_id: "user-1".into(),
                        profile_key: LOCAL_SKILL_BUNDLE_PROFILE_KEY.into(),
                        capability_policy_revision: "policy-1".into(),
                        instructions: Some(encoded),
                        prefixed_input_items: vec![],
                        tools: vec![],
                    },
                }),
            ))
            .await
            .expect("publish bundle");
        runtime
            .try_handle(envelope(
                "create",
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-skill".into(),
                    owner_user_id: "user-1".into(),
                    owner_entity_type: "conversation".into(),
                    owner_entity_id: "conversation-1".into(),
                    profile_key: "main_chat".into(),
                    model_config_ref: "model-1".into(),
                    model_config_revision: "revision-1".into(),
                    capability_policy_revision: "policy-1".into(),
                    input: json!({"message": "hello"}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create run");

        let denied = runtime
            .authorize_tool_skill_for_run("run-skill", "create_task")
            .await
            .expect("gate")
            .expect_err("activation required");
        assert_eq!(denied.required_skill_refs, ["product-skill:task"]);

        let claim = runtime
            .try_handle(envelope(
                "claim",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "model-worker".into(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .expect("claim run");
        let HostResult::Claim { claim: Some(claim) } = claim else {
            panic!("expected run claim")
        };
        runtime
            .try_handle(envelope(
                "wait",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".into(),
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: "batch-1".into(),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: "call-activate".into(),
                            tool_name: LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME.into(),
                            arguments: json!({"skill_ref": "product-skill:task"}),
                            side_effecting: false,
                            requires_approval: false,
                        }],
                        checkpoint: json!({"response_id": "response-1"}),
                    },
                }),
            ))
            .await
            .expect("queue activation");

        let executor =
            LocalSkillToolExecutor::new(Arc::clone(&runtime), "user-1").expect("skill executor");
        let mut tools = LocalToolRegistry::new();
        executor.register_into(&mut tools).expect("register tools");
        let scheduler =
            LocalToolScheduler::new(Arc::clone(&runtime), tools, "user-1", "tool-worker")
                .expect("scheduler");
        let ToolSchedulerTick::Committed(activation) =
            scheduler.run_once().await.expect("execute activation")
        else {
            panic!("expected activation commit")
        };
        assert_eq!(
            activation.invocation.execution_route,
            chatos_local_agent_protocol::LocalAgentToolExecutionRoute::Shared
        );
        assert!(runtime
            .authorize_tool_skill_for_run("run-skill", "create_task")
            .await
            .expect("gate")
            .is_ok());

        let restarted = Arc::new(LocalAgentRuntime::new(storage));
        restarted.initialize("user-1").await.expect("restart");
        assert_eq!(
            restarted
                .activated_skill_refs_for_run("user-1", "run-skill")
                .await
                .expect("durable activations"),
            ["product-skill:task"]
        );
        assert!(restarted
            .authorize_tool_skill_for_run("run-skill", "create_task")
            .await
            .expect("gate after restart")
            .is_ok());

        let mut read = activation.invocation.clone();
        read.tool_name = LOCAL_AGENT_SKILL_READ_RESOURCE_TOOL_NAME.into();
        read.arguments = json!({
            "skill_ref": "product-skill:task",
            "relative_path": "references/example.md",
            "offset": 1,
            "max_chars": 2
        });
        let restarted_executor =
            LocalSkillToolExecutor::new(restarted, "user-1").expect("restarted executor");
        let page = restarted_executor
            .execute(&read)
            .await
            .expect("resource page");
        assert_eq!(page["content"], "乙丙");
        assert_eq!(page["next_offset"], 3);
        assert_eq!(page["truncated"], true);
    }

    #[tokio::test]
    async fn run_without_a_skill_bundle_has_no_skill_gate() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::new(storage);
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(envelope(
                "legacy-create",
                HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-legacy".into(),
                    owner_user_id: "user-1".into(),
                    owner_entity_type: "conversation".into(),
                    owner_entity_id: "conversation-1".into(),
                    profile_key: "main_chat".into(),
                    model_config_ref: "model-1".into(),
                    model_config_revision: "revision-1".into(),
                    capability_policy_revision: "old-policy".into(),
                    input: json!({}),
                    max_iterations: 4,
                }),
            ))
            .await
            .expect("create legacy run");
        assert!(runtime
            .authorize_tool_skill_for_run("run-legacy", "create_task")
            .await
            .expect("ungated profile")
            .is_ok());
        assert_eq!(
            runtime
                .try_handle(envelope(
                    "legacy-skill-bundle",
                    HostCommand::GetRunSkillBundle(GetRunCommand {
                        owner_user_id: "user-1".into(),
                        run_id: "run-legacy".into(),
                    }),
                ))
                .await
                .expect("legacy bundle query"),
            HostResult::RunSkillBundle {
                run_id: "run-legacy".into(),
                bundle: None
            }
        );
    }
}
