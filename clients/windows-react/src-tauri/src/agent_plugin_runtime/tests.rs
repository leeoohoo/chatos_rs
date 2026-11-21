use super::*;

fn context() -> AgentPluginRunContext {
    AgentPluginRunContext {
        owner_user_id: "owner-1".into(),
        run_id: "run-1".into(),
        execution_root: PathBuf::from("/project"),
        workspace_scope_id: "workspace-1".into(),
        input: json!({
            "team_executor_context": {
                "execution_plan": {
                    "plugins": [{
                        "installation_id": "installation-1",
                        "plugin_id": "document-plugin",
                        "component_key": "document-mcp",
                        "component_revision": "revision-1",
                        "release_id": "release-1",
                        "release_digest": format!("sha256:{}", "a".repeat(64)),
                        "manifest_version": "1.2.3",
                        "display_name": "Document Tools",
                        "reason": "Create a document"
                    }]
                }
            }
        }),
    }
}

#[test]
fn frozen_plan_parser_rejects_unknown_fields_and_opaque_options_are_run_scoped() {
    let first = context();
    let selection = frozen_plugin_selections(&first).unwrap().remove(0);
    let first_option = plugin_option(&first, &selection);
    let mut second = first.clone();
    second.run_id = "run-2".into();
    assert_ne!(first_option, plugin_option(&second, &selection));
    assert!(!first_option.contains(&selection.plugin_id));

    let mut forged = first;
    forged.input["team_executor_context"]["execution_plan"]["plugins"][0]["path"] =
        json!("C:\\private\\plugin.exe");
    assert!(frozen_plugin_selections(&forged).is_err());
}

#[tokio::test]
async fn empty_frozen_plugin_plan_returns_an_empty_search_catalog() {
    let mut context = context();
    context.input["team_executor_context"]["execution_plan"]["plugins"] = json!([]);
    assert!(frozen_plugin_selections(&context).unwrap().is_empty());

    let directory = tempfile::tempdir().unwrap();
    let app_data = directory.path().join("app-data");
    let manager = PluginApplicationManager::new(app_data.clone());
    let host = HostManager::default();
    let runtime = AgentPluginRuntime::new(
        directory.path().join("plugins"),
        directory.path().join("journal"),
    );
    let outcome = runtime
        .execute(
            &manager,
            &host,
            context,
            "invocation-1",
            CAPABILITY_SEARCH_TOOL_NAME,
            &Map::from_iter([("query".into(), json!("document"))]),
        )
        .await
        .unwrap();
    let LocalAgentToolOutcome::Succeeded { output } = outcome else {
        panic!("search outcome")
    };
    assert_eq!(output["matches"], json!([]));
}

#[tokio::test]
async fn journal_replays_completed_results_and_never_replays_running_intents() {
    let directory = tempfile::tempdir().unwrap();
    let journal = InvocationJournal::new(directory.path().join("journal"));
    assert!(matches!(
        journal
            .begin("owner-1", "run-1", "call-1", "sig-1")
            .await
            .unwrap(),
        JournalBegin::Execute
    ));
    assert!(matches!(
        journal
            .begin("owner-1", "run-1", "call-1", "sig-1")
            .await
            .unwrap(),
        JournalBegin::Uncertain
    ));
    let outcome = LocalAgentToolOutcome::Succeeded {
        output: json!({"ok": true}),
    };
    journal
        .complete("owner-1", "run-1", "call-1", "sig-1", outcome.clone())
        .await
        .unwrap();
    match journal
        .begin("owner-1", "run-1", "call-1", "sig-1")
        .await
        .unwrap()
    {
        JournalBegin::Replay(replayed) => assert_eq!(replayed, outcome),
        _ => panic!("expected durable replay"),
    }
    assert!(journal
        .begin("owner-1", "run-1", "call-1", "changed")
        .await
        .is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let root_mode = std::fs::metadata(&journal.root)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(root_mode, 0o700);
        let base = journal.path("owner-1", "run-1", "call-1").await.unwrap();
        let result_mode = std::fs::metadata(journal::journal_stage_path(&base, "result"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(result_mode, 0o600);
    }
}

#[test]
fn journal_cleanup_removes_only_stale_canonical_temporaries_within_bound() {
    let directory = tempfile::tempdir().unwrap();
    let token = "00000000-0000-0000-0000-000000000000";
    let canonical = directory.path().join(format!(".{token}.tmp"));
    let malformed = directory.path().join(".not-a-uuid.tmp");
    std::fs::write(&canonical, b"stale").unwrap();
    std::fs::write(&malformed, b"keep").unwrap();

    assert_eq!(
        cleanup_stale_journal_temporaries_before(directory.path(), UNIX_EPOCH, 10),
        0
    );
    assert!(canonical.exists());
    assert!(malformed.exists());

    assert_eq!(
        cleanup_stale_journal_temporaries_before(
            directory.path(),
            SystemTime::now() + Duration::from_secs(1),
            10,
        ),
        1
    );
    assert!(!canonical.exists());
    assert!(malformed.exists());

    let bounded = (1..=3)
        .map(|index| {
            let path = directory
                .path()
                .join(format!(".00000000-0000-0000-0000-00000000000{index}.tmp"));
            std::fs::write(&path, b"stale").unwrap();
            path
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cleanup_stale_journal_temporaries_before(
            directory.path(),
            SystemTime::now() + Duration::from_secs(1),
            2,
        ),
        2
    );
    assert_eq!(bounded.iter().filter(|path| path.exists()).count(), 1);
}

#[tokio::test]
async fn journal_stage_records_are_append_only() {
    let directory = tempfile::tempdir().unwrap();
    let journal = InvocationJournal::new(directory.path().join("journal"));
    journal
        .begin("owner-1", "run-1", "call-1", "sig-1")
        .await
        .unwrap();
    journal
        .complete(
            "owner-1",
            "run-1",
            "call-1",
            "sig-1",
            LocalAgentToolOutcome::Succeeded {
                output: json!({"value": 1}),
            },
        )
        .await
        .unwrap();
    assert!(journal
        .complete(
            "owner-1",
            "run-1",
            "call-1",
            "sig-1",
            LocalAgentToolOutcome::Succeeded {
                output: json!({"value": 2}),
            },
        )
        .await
        .is_err());
    match journal
        .begin("owner-1", "run-1", "call-1", "sig-1")
        .await
        .unwrap()
    {
        JournalBegin::Replay(LocalAgentToolOutcome::Succeeded { output }) => {
            assert_eq!(output, json!({"value": 1}));
        }
        _ => panic!("expected the original append-only result"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn journal_rejects_a_symlinked_root() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let actual = directory.path().join("actual");
    let linked = directory.path().join("journal");
    std::fs::create_dir(&actual).unwrap();
    symlink(&actual, &linked).unwrap();
    let journal = InvocationJournal::new(linked);
    assert!(journal
        .begin("owner-1", "run-1", "call-1", "sig-1")
        .await
        .is_err());
}

#[test]
fn broker_catalog_keeps_invoke_out_of_the_read_only_surface() {
    let schemas = capability_broker_tool_schemas();
    assert_eq!(schemas.len(), 5);
    assert_eq!(schemas[0]["name"], CAPABILITY_SEARCH_TOOL_NAME);
    assert_eq!(schemas[1]["name"], CAPABILITY_DESCRIBE_TOOL_NAME);
    assert_eq!(schemas[2]["name"], AGENT_SKILL_ACTIVATE_TOOL_NAME);
    assert_eq!(schemas[3]["name"], AGENT_SKILL_READ_RESOURCE_TOOL_NAME);
    assert_eq!(schemas[4]["name"], CAPABILITY_INVOKE_TOOL_NAME);
}

#[test]
fn skill_snapshot_discovers_only_bounded_bundled_skill_documents() {
    let directory = tempfile::tempdir().unwrap();
    let installation = directory.path().join("plugin");
    let skill = installation.join("skills").join("safe-helper");
    std::fs::create_dir_all(skill.join("references").join("nested")).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: safe-helper\ndescription: Safe helper\nchatos.role: router\n---\nFollow the frozen instructions.\n",
    )
    .unwrap();
    std::fs::write(
        skill.join("references").join("nested").join("guide.md"),
        "guide",
    )
    .unwrap();

    let snapshot =
        load_agent_plugin_skill_snapshot(&installation, "plugin-package", std::iter::empty())
            .unwrap();

    let document = snapshot.documents.get("safe-helper").unwrap();
    assert_eq!(document.role, "router");
    assert_eq!(
        document.resources.get("references/nested/guide.md"),
        Some(&"guide".to_owned())
    );
    assert!(load_agent_plugin_skill_snapshot(
        &installation,
        "plugin-package",
        ["../outside"].into_iter(),
    )
    .is_err());
}

#[tokio::test]
async fn skill_only_session_never_starts_a_process_and_preserves_disclosure_gates() {
    let directory = tempfile::tempdir().unwrap();
    let plugin_root = directory.path().join("Plugins");
    let installation = plugin_root.join("plugin-1").join("1.0.0");
    let skill = installation.join("skills").join("portable-helper");
    for resource in ["scripts", "references", "assets"] {
        std::fs::create_dir_all(skill.join(resource)).unwrap();
    }
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: portable-helper\ndescription: Portable helper\n---\nFollow the portable rules.\n",
    )
    .unwrap();
    std::fs::write(skill.join("scripts/check.ps1"), "Write-Output safe").unwrap();
    std::fs::write(skill.join("references/guide.md"), "Guide").unwrap();
    std::fs::write(skill.join("assets/readme.txt"), "Asset").unwrap();
    let context = context();
    let snapshot = load_agent_plugin_skill_snapshot(
        &installation,
        "portable-helper",
        ["skills/portable-helper"].into_iter(),
    )
    .unwrap();
    let resolved = ResolvedAgentPluginExecution {
        identity: frozen_plugin_selections(&context).unwrap().remove(0),
        installation_root: installation.clone(),
        executable: None,
        args: Vec::new(),
        working_directory: Some(installation),
        environment: BTreeMap::new(),
        network_policy: "disabled",
        allowed_tools: None,
        skill_snapshot: snapshot,
    };
    let fingerprint = resolved_runtime_fingerprint(&resolved).unwrap();

    let _manager = PluginApplicationManager::new(directory.path().join("app-data"));
    let session = McpSession::connect(
        &plugin_root,
        &context,
        resolved,
        fingerprint,
        None,
        #[cfg(windows)]
        _manager.workspace_leases(),
    )
    .await
    .unwrap();

    assert!(session.connection.is_none());
    assert!(session.definitions.is_empty());
    assert_eq!(session.skills_by_option.len(), 1);
    let skill_ref = session.skills_by_option.keys().next().unwrap().clone();
    assert!(session.activate_skill(&skill_ref).await.is_err());
    let (catalog, next_offset) = session
        .describe_skill_catalog("portable", 0, 8)
        .await
        .unwrap();
    assert_eq!(catalog[0]["skill_ref"], skill_ref);
    assert_eq!(catalog[0]["name"], "portable-helper");
    assert_eq!(next_offset, None);
    let activated = session.activate_skill(&skill_ref).await.unwrap().unwrap();
    assert!(activated.resources.contains_key("scripts/check.ps1"));
    assert!(activated.resources.contains_key("references/guide.md"));
    assert!(activated.resources.contains_key("assets/readme.txt"));
    let error = session.call_tool("forged", json!({})).await.unwrap_err();
    assert!(error.outcome_known);
    session.invalidate().await;
}

#[cfg(unix)]
#[test]
fn skill_snapshot_rejects_a_symlinked_declared_directory() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let installation = directory.path().join("plugin");
    let outside = directory.path().join("outside");
    std::fs::create_dir_all(&installation).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(
        outside.join("SKILL.md"),
        "---\nname: outside\ndescription: Outside\n---\nDo not load.\n",
    )
    .unwrap();
    symlink(&outside, installation.join("linked-skill")).unwrap();

    assert!(load_agent_plugin_skill_snapshot(
        &installation,
        "plugin-package",
        ["linked-skill"].into_iter(),
    )
    .is_err());
}

#[tokio::test]
async fn stdio_session_initializes_lists_and_calls_a_real_plugin_process() {
    let directory = tempfile::tempdir().unwrap();
    let plugin_root = directory.path().join("Plugins");
    let installation = plugin_root.join("plugin-1").join("1.0.0");
    let project_root = directory.path().join("project");
    std::fs::create_dir_all(&installation).unwrap();
    std::fs::create_dir_all(&project_root).unwrap();
    let skill_directory = installation.join("skills").join("document-helper");
    std::fs::create_dir_all(skill_directory.join("references")).unwrap();
    std::fs::write(
        skill_directory.join("SKILL.md"),
        "---\nname: document-helper\ndescription: Safe document rules\nchatos.role: leaf\n---\nUse the document tool carefully.\n",
    )
    .unwrap();
    std::fs::write(
        skill_directory.join("references").join("guide.md"),
        "Reference content.",
    )
    .unwrap();
    let executable = installation.join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    let fixture_source = installation.join("fixture.rs");
    std::fs::write(
        &fixture_source,
        r###"
use std::{env, fs, io::{self, BufRead, Write}, path::PathBuf};

fn main() {
let stdin = io::stdin();
let mut stdout = io::stdout().lock();
for line in stdin.lock().lines() {
    let line = line.unwrap();
    let Some(id_tail) = line.split("\"id\":").nth(1) else { continue };
    let id = id_tail.chars().take_while(|value| value.is_ascii_digit()).collect::<String>();
    let response = if line.contains("\"method\":\"initialize\"") {
        r#"{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}}"#.to_owned()
    } else if line.contains("\"method\":\"tools/list\"") {
        r#"{"jsonrpc":"2.0","id":0,"result":{"tools":[{"name":"echo_document","description":"Echo a bounded document request.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false},"_meta":{"chatos/skillGate":{"allOf":["document-helper"]}}}]}}"#.to_owned()
    } else if line.contains("\"method\":\"tools/call\"") {
        let workspace = PathBuf::from(env::var_os("CHATOS_WORKSPACE_ROOT").unwrap());
        let workspace_written = fs::write(workspace.join("plugin-output.txt"), b"verified").is_ok();
        let installation = env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let installation_write_denied = fs::write(installation.join("plugin-escape.txt"), b"escape").is_err();
        format!(r#"{{"jsonrpc":"2.0","id":0,"result":{{"content":[{{"type":"text","text":"verified"}},{{"type":"image","data":"/9j/2Q==","mimeType":"image/jpeg"}}],"structuredContent":{{"captureRegionGlobal":{{"x":0,"y":0,"width":1,"height":1}},"workspaceWritten":{workspace_written},"installationWriteDenied":{installation_write_denied}}},"isError":false}}}}"#)
    } else {
        continue;
    };
    writeln!(stdout, "{}", response.replacen("\"id\":0", &format!("\"id\":{id}"), 1)).unwrap();
    stdout.flush().unwrap();
}
}
"###,
    )
    .unwrap();
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = std::process::Command::new(compiler)
        .arg(&fixture_source)
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap();
    assert!(compiled.success());
    let context = AgentPluginRunContext {
        execution_root: project_root,
        ..context()
    };
    let skill_snapshot = load_agent_plugin_skill_snapshot(
        &installation,
        "plugin-package",
        ["./skills/document-helper"].into_iter(),
    )
    .unwrap();
    let resolved = ResolvedAgentPluginExecution {
        identity: frozen_plugin_selections(&context).unwrap().remove(0),
        installation_root: installation.clone(),
        executable: Some(executable),
        args: Vec::new(),
        working_directory: Some(installation.clone()),
        environment: BTreeMap::new(),
        network_policy: "disabled",
        allowed_tools: Some(vec!["echo_document".into()]),
        skill_snapshot,
    };

    let fingerprint = resolved_runtime_fingerprint(&resolved).unwrap();
    let _app_data = directory.path().join("app-data");
    let _manager = PluginApplicationManager::new(_app_data.clone());
    let resources = MachineResourceGovernor::default();
    let permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await
        .unwrap();
    let session = McpSession::connect(
        &plugin_root,
        &context,
        resolved,
        fingerprint,
        Some(permit),
        #[cfg(windows)]
        _manager.workspace_leases(),
    )
    .await
    .unwrap();
    assert_eq!(resources.used_resource_units(), 1);

    assert_eq!(session.definitions.len(), 1);
    assert_eq!(session.definitions[0].name, "echo_document");
    let definition = session.definitions[0].clone();
    assert_eq!(definition.required_skill_refs.len(), 1);
    let skill_ref = definition.required_skill_refs[0].clone();
    assert!(session.activate_skill(&skill_ref).await.is_err());

    session
        .restore_disclosure(&definition.option, &"0".repeat(64))
        .await;
    assert!(!session.was_described(&definition.option).await);
    assert!(session.activate_skill(&skill_ref).await.is_err());

    session
        .restore_skill_activations(&["forged-skill-ref".into()])
        .await;
    let missing = session
        .missing_skill_refs(&definition, &json!({"text": "verified"}))
        .await
        .unwrap();
    assert_eq!(missing.as_slice(), std::slice::from_ref(&skill_ref));
    session
        .restore_disclosure(&definition.option, &definition.schema_sha256)
        .await;
    assert!(session.was_described(&definition.option).await);
    session
        .restore_skill_activations(&[skill_ref.clone(), "forged-skill-ref".into()])
        .await;
    assert!(session.activated_skills.lock().await.contains(&skill_ref));
    assert!(!session
        .activated_skills
        .lock()
        .await
        .contains("forged-skill-ref"));
    let activated = session.activate_skill(&skill_ref).await.unwrap().unwrap();
    assert!(activated
        .instructions
        .contains("Use the document tool carefully."));
    assert_eq!(
        activated.resources.keys().collect::<Vec<_>>(),
        ["references/guide.md"]
    );
    let page = session
        .read_skill_resource(&skill_ref, "references/guide.md", 0, 9)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(page.content, "Reference");
    assert_eq!(page.next_offset, Some(9));
    assert!(session
        .missing_skill_refs(&definition, &json!({"text": "verified"}))
        .await
        .unwrap()
        .is_empty());
    let result = session
        .call_tool("echo_document", json!({"text": "verified"}))
        .await
        .unwrap();
    assert_eq!(result["content"][0]["text"], "verified");
    assert_eq!(result["content"][1]["type"], "image");
    assert_eq!(result["content"][1]["mimeType"], "image/jpeg");
    assert_eq!(result["content"][1]["data"], "/9j/2Q==");
    assert_eq!(
        result["structuredContent"]["captureRegionGlobal"]["width"],
        1
    );
    assert_eq!(result["structuredContent"]["workspaceWritten"], true);
    assert_eq!(
        std::fs::read(context.execution_root.join("plugin-output.txt")).unwrap(),
        b"verified"
    );
    #[cfg(windows)]
    {
        assert_eq!(result["structuredContent"]["installationWriteDenied"], true);
        assert!(!installation.join("plugin-escape.txt").exists());
    }
    #[cfg(not(windows))]
    let _ = std::fs::remove_file(installation.join("plugin-escape.txt"));
    session.invalidate().await;
    assert_eq!(resources.used_resource_units(), 0);
    #[cfg(windows)]
    for journal in [
        _app_data
            .join("local-agent")
            .join("appcontainer-workspace-leases"),
        _app_data
            .join("local-agent")
            .join("appcontainer-profile-leases"),
    ] {
        assert_eq!(std::fs::read_dir(journal).unwrap().count(), 0);
    }
}
