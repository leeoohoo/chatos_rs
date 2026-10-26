use super::*;
use chatos_local_agent_protocol::{LocalPluginInstallationPage, LocalPluginInstallationSpec};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    sync::Mutex,
};

struct ScriptedMetadataClient {
    responses: Mutex<VecDeque<Result<HostResult, String>>>,
}

impl ScriptedMetadataClient {
    fn new(responses: Vec<Result<HostResult, String>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }

    fn remaining(&self) -> usize {
        self.responses.lock().unwrap().len()
    }
}

impl PluginInstallationMetadataClient for ScriptedMetadataClient {
    fn execute(&self, _command: HostCommand) -> MetadataFuture<'_> {
        Box::pin(async move {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected Plugin metadata request")
        })
    }
}

struct CatalogFixture {
    _directory: tempfile::TempDir,
    app_data: PathBuf,
    installation: PathBuf,
    executable: PathBuf,
}

impl CatalogFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let installation = app_data
            .join("NativeConnector")
            .join("Plugins")
            .join("plugin-1")
            .join("1.0.0");
        let executable = installation.join("bin").join("plugin-host.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::create_dir_all(installation.join("ui")).unwrap();
        fs::write(&executable, "host").unwrap();
        fs::write(installation.join("ui/index.html"), "<main>Plugin</main>").unwrap();
        let fixture = Self {
            _directory: directory,
            app_data,
            installation,
            executable,
        };
        fixture.write_manifest(Self::valid_manifest());
        fixture
    }

    fn valid_manifest() -> Value {
        json!({
            "schemaVersion": 3,
            "name": "plugin-package",
            "version": "1.0.0",
            "description": "A safe Windows Plugin application.",
            "mcpServers": {
                "server-1": {
                    "type": "stdio",
                    "bin": "plugin-host",
                    "args": ["mcp"],
                    "env": {},
                    "agent": {
                        "displayName": "Plugin Agent Tools",
                        "description": "Trusted tools for the current project."
                    }
                }
            },
            "ui": [{
                "componentKey": "plugin-workbench",
                "source": "./ui/index.html",
                "title": "Plugin Workbench",
                "surface": "workbench",
                "bridgeCapabilities": ["host.context.read"],
                "runtime": {
                    "type": "local_http",
                    "bin": "plugin-host",
                    "args": ["studio"],
                    "healthPath": "/api/health",
                    "launchTimeoutMs": 15000
                }
            }],
            "interface": {
                "displayName": "Plugin",
                "brandColor": "#007AFF"
            },
            "dependencies": {
                "supportedPlatforms": ["macos", "windows"]
            },
            "runtimeContext": {
                "scope": "project",
                "components": ["plugin-workbench"],
                "required": [],
                "optional": ["project.id", "workspace.id", "workspace.root"],
                "storageIsolation": "project",
                "missingContext": "device"
            },
            "permissions": [{
                "permission": "process.spawn",
                "required": true,
                "components": ["plugin-workbench", "server-1"]
            }]
        })
    }

    fn write_manifest(&self, manifest: Value) {
        fs::write(
            self.installation.join("chatos.plugin.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn record(&self) -> LocalPluginInstallationRecord {
        LocalPluginInstallationRecord {
            spec: LocalPluginInstallationSpec {
                installation_id: "installation-1".into(),
                owner_user_id: "owner-1".into(),
                plugin_id: "plugin-1".into(),
                release_id: "release-1".into(),
                release_digest: format!("sha256:{}", "a".repeat(64)),
                component_id: "component-1".into(),
                component_revision: "revision-1".into(),
                server_id: "server-1".into(),
                executable_path: self.executable.to_string_lossy().into_owned(),
                args: Vec::new(),
                working_directory: None,
                environment_secret_refs: BTreeMap::new(),
                tool_prefix: None,
                allowed_tools: None,
                enabled: true,
            },
            version: 1,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
        }
    }
}

fn summary(record: &LocalPluginInstallationRecord) -> LocalPluginInstallationSummary {
    LocalPluginInstallationSummary {
        installation_id: record.spec.installation_id.clone(),
        owner_user_id: record.spec.owner_user_id.clone(),
        plugin_id: record.spec.plugin_id.clone(),
        release_id: record.spec.release_id.clone(),
        component_id: record.spec.component_id.clone(),
        component_revision: record.spec.component_revision.clone(),
        server_id: record.spec.server_id.clone(),
        enabled: record.spec.enabled,
        version: record.version,
        created_at_unix_ms: record.created_at_unix_ms,
        updated_at_unix_ms: record.updated_at_unix_ms,
    }
}

fn page(
    installations: Vec<LocalPluginInstallationSummary>,
    cursor: Option<(i64, &str)>,
) -> HostResult {
    HostResult::PluginInstallations {
        page: LocalPluginInstallationPage {
            installations,
            next_before_updated_at_unix_ms: cursor.map(|value| value.0),
            next_before_installation_id: cursor.map(|value| value.1.to_owned()),
        },
    }
}

fn record_result(record: LocalPluginInstallationRecord) -> HostResult {
    HostResult::PluginInstallation {
        installation: record,
    }
}

fn catalog_client(record: &LocalPluginInstallationRecord) -> ScriptedMetadataClient {
    ScriptedMetadataClient::new(vec![
        Ok(page(vec![summary(record)], None)),
        Ok(record_result(record.clone())),
    ])
}

fn frozen_agent_option(record: &LocalPluginInstallationRecord) -> TrustedAgentPluginOption {
    TrustedAgentPluginOption {
        installation_id: record.spec.installation_id.clone(),
        plugin_id: record.spec.plugin_id.clone(),
        component_key: record.spec.server_id.clone(),
        component_revision: record.spec.component_revision.clone(),
        release_id: record.spec.release_id.clone(),
        release_digest: record.spec.release_digest.clone(),
        manifest_version: "1.0.0".into(),
        display_name: "Plugin Agent Tools".into(),
        description: "Trusted tools for the current project.".into(),
    }
}

#[tokio::test]
async fn catalog_projects_only_valid_owner_scoped_metadata_without_local_paths() {
    let fixture = CatalogFixture::new();
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert_eq!(applications.len(), 1);
    assert_eq!(applications[0].plugin_id, "plugin-1");
    assert_eq!(applications[0].component_key, "plugin-workbench");
    assert_eq!(applications[0].display_name, "Plugin Workbench");
    assert!(applications[0].requires_local_runtime);
    assert_eq!(applications[0].context_scope.as_deref(), Some("project"));
    assert_eq!(applications[0].missing_context.as_deref(), Some("device"));
    assert_eq!(applications[0].bridge_capabilities, ["host.context.read"]);
    let projection = serde_json::to_string(&applications).unwrap();
    assert!(!projection.contains(&fixture.app_data.to_string_lossy().to_string()));
    assert!(!projection.contains("executablePath"));
    assert!(!projection.contains("installationPath"));
    assert!(!projection.contains("source"));
    assert_eq!(client.remaining(), 0);
}

#[tokio::test]
async fn agent_catalog_freezes_only_manifest_backed_stdio_components() {
    let fixture = CatalogFixture::new();
    let mut record = fixture.record();
    record.spec.args = vec!["mcp".into()];
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let plugins = manager
        .list_agent_plugins(&client, "owner-1")
        .await
        .unwrap();

    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].installation_id, "installation-1");
    assert_eq!(plugins[0].plugin_id, "plugin-1");
    assert_eq!(plugins[0].component_key, "server-1");
    assert_eq!(plugins[0].component_revision, "revision-1");
    assert_eq!(plugins[0].release_id, "release-1");
    assert_eq!(plugins[0].manifest_version, "1.0.0");
    assert_eq!(plugins[0].display_name, "Plugin Agent Tools");
    assert_eq!(plugins[0].selection_id(), "plugin-1\0server-1");
    assert!(!plugins[0]
        .frozen_identity()
        .contains(&fixture.app_data.to_string_lossy().to_string()));
    assert_eq!(client.remaining(), 0);
}

#[tokio::test]
async fn skill_only_plugin_is_selectable_and_resolves_without_an_executable() {
    let fixture = CatalogFixture::new();
    let skill = fixture.installation.join("skills").join("portable-helper");
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: portable-helper\ndescription: Portable helper rules\n---\nFollow the portable rules.\n",
    )
    .unwrap();
    fs::write(skill.join("references/guide.md"), "Guide").unwrap();
    fixture.write_manifest(json!({
        "schemaVersion": 3,
        "name": "portable-helper",
        "version": "1.0.0",
        "description": "Portable helper rules.",
        "skills": ["skills/portable-helper"],
        "mcpServers": {},
        "interface": {"displayName": "Portable Helper"},
        "permissions": []
    }));
    let mut record = fixture.record();
    record.spec.component_id = "skills-only".into();
    record.spec.server_id = "skills-only".into();
    record.spec.executable_path = fixture
        .installation
        .join("chatos.plugin.json")
        .to_string_lossy()
        .into_owned();
    record.spec.args.clear();
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let options = manager
        .list_agent_plugins(&catalog_client(&record), "owner-1")
        .await
        .unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].component_key, "skills-only");
    assert_eq!(options[0].display_name, "Portable Helper");

    let client = ScriptedMetadataClient::new(vec![Ok(record_result(record))]);
    let resolved = manager
        .resolve_agent_plugin(&client, "owner-1", &options[0])
        .await
        .unwrap();
    assert!(resolved.executable.is_none());
    assert!(resolved.args.is_empty());
    assert!(resolved.environment.is_empty());
    assert_eq!(resolved.skill_snapshot.len(), 1);
}

#[tokio::test]
async fn agent_catalog_rejects_missing_or_non_stdio_manifest_components() {
    let fixture = CatalogFixture::new();
    let mut record = fixture.record();
    record.spec.args = vec!["mcp".into()];
    let manager = PluginApplicationManager::new(fixture.app_data.clone());
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["mcpServers"]["server-1"]["type"] = json!("http");
    fixture.write_manifest(manifest);

    let plugins = manager
        .list_agent_plugins(&catalog_client(&record), "owner-1")
        .await
        .unwrap();

    assert!(plugins.is_empty());
}

#[tokio::test]
async fn agent_catalog_hides_unresolvable_secret_or_argument_bindings() {
    let fixture = CatalogFixture::new();
    let mut secret_record = fixture.record();
    secret_record.spec.args = vec!["mcp".into()];
    secret_record
        .spec
        .environment_secret_refs
        .insert("API_TOKEN".into(), "credential-1".into());
    let manager = PluginApplicationManager::new(fixture.app_data.clone());
    assert!(manager
        .list_agent_plugins(&catalog_client(&secret_record), "owner-1")
        .await
        .unwrap()
        .is_empty());

    let mut drifted_record = fixture.record();
    drifted_record.spec.args = vec!["different-mode".into()];
    assert!(manager
        .list_agent_plugins(&catalog_client(&drifted_record), "owner-1")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn agent_runtime_resolves_only_the_exact_frozen_installation() {
    let fixture = CatalogFixture::new();
    let skill = fixture.installation.join("skills").join("plugin-helper");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: plugin-helper\ndescription: Plugin helper\n---\nFollow the Plugin rules.\n",
    )
    .unwrap();
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["skills"] = json!(["./skills/plugin-helper"]);
    fixture.write_manifest(manifest);
    let mut record = fixture.record();
    record.spec.args = vec!["mcp".into()];
    let expected = frozen_agent_option(&record);
    let client = ScriptedMetadataClient::new(vec![Ok(record_result(record))]);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let resolved = manager
        .resolve_agent_plugin(&client, "owner-1", &expected)
        .await
        .unwrap();

    assert_eq!(resolved.identity, expected);
    assert_eq!(resolved.executable, Some(fixture.executable));
    assert_eq!(resolved.args, ["mcp"]);
    assert!(resolved.environment.is_empty());
    assert_eq!(resolved.skill_snapshot.len(), 1);
    assert_eq!(client.remaining(), 0);
}

#[tokio::test]
async fn agent_runtime_rejects_component_release_or_manifest_drift() {
    for drift in ["component", "release", "manifest", "skill"] {
        let fixture = CatalogFixture::new();
        let mut record = fixture.record();
        record.spec.args = vec!["mcp".into()];
        let expected = frozen_agent_option(&record);
        match drift {
            "component" => record.spec.component_revision = "revision-2".into(),
            "release" => record.spec.release_id = "release-2".into(),
            "manifest" => {
                let mut manifest = CatalogFixture::valid_manifest();
                manifest["version"] = json!("2.0.0");
                fixture.write_manifest(manifest);
            }
            "skill" => {
                let mut manifest = CatalogFixture::valid_manifest();
                manifest["skills"] = json!(["./skills/missing"]);
                fixture.write_manifest(manifest);
            }
            _ => unreachable!(),
        }
        let client = ScriptedMetadataClient::new(vec![Ok(record_result(record))]);
        let manager = PluginApplicationManager::new(fixture.app_data.clone());

        assert!(manager
            .resolve_agent_plugin(&client, "owner-1", &expected)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn agent_runtime_fails_closed_without_a_windows_secret_resolver() {
    let fixture = CatalogFixture::new();
    let mut record = fixture.record();
    record.spec.args = vec!["mcp".into()];
    let expected = frozen_agent_option(&record);
    record
        .spec
        .environment_secret_refs
        .insert("API_TOKEN".into(), "credential-1".into());
    let client = ScriptedMetadataClient::new(vec![Ok(record_result(record))]);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    assert!(manager
        .resolve_agent_plugin(&client, "owner-1", &expected)
        .await
        .is_err());
}

#[tokio::test]
async fn agent_runtime_rejects_arguments_that_drift_from_the_manifest() {
    let fixture = CatalogFixture::new();
    let mut record = fixture.record();
    record.spec.args = vec!["different-mode".into()];
    let expected = frozen_agent_option(&record);
    let client = ScriptedMetadataClient::new(vec![Ok(record_result(record))]);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    assert!(manager
        .resolve_agent_plugin(&client, "owner-1", &expected)
        .await
        .is_err());
}

#[tokio::test]
async fn catalog_rejects_cross_owner_host_records() {
    let fixture = CatalogFixture::new();
    let record = fixture.record();
    let mut foreign = summary(&record);
    foreign.owner_user_id = "owner-2".into();
    let client = ScriptedMetadataClient::new(vec![Ok(page(vec![foreign], None))]);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let error = manager.list(&client, "owner-1").await.unwrap_err();

    assert_eq!(error, metadata_response_error());
    assert_eq!(client.remaining(), 0);
}

#[tokio::test]
async fn installation_record_must_match_the_listed_revision_exactly() {
    let fixture = CatalogFixture::new();
    let mut record = fixture.record();
    let listed = summary(&record);
    record.spec.component_revision = "substituted-revision".into();
    let client = ScriptedMetadataClient::new(vec![
        Ok(page(vec![listed], None)),
        Ok(record_result(record)),
    ]);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[tokio::test]
async fn pagination_rejects_partial_and_repeated_cursors() {
    let partial = ScriptedMetadataClient::new(vec![Ok(HostResult::PluginInstallations {
        page: LocalPluginInstallationPage {
            installations: Vec::new(),
            next_before_updated_at_unix_ms: Some(20),
            next_before_installation_id: None,
        },
    })]);
    assert!(list_installation_summaries(&partial, "owner-1")
        .await
        .unwrap_err()
        .contains("无效"));

    let repeated = ScriptedMetadataClient::new(vec![
        Ok(page(Vec::new(), Some((20, "installation-1")))),
        Ok(page(Vec::new(), Some((20, "installation-1")))),
    ]);
    assert!(list_installation_summaries(&repeated, "owner-1")
        .await
        .unwrap_err()
        .contains("重复"));
}

#[tokio::test]
async fn catalog_ignores_an_executable_outside_the_plugin_root() {
    let fixture = CatalogFixture::new();
    let outside = fixture.app_data.join("outside.exe");
    fs::write(&outside, "outside").unwrap();
    let mut record = fixture.record();
    record.spec.executable_path = outside.to_string_lossy().into_owned();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[tokio::test]
async fn catalog_ignores_source_path_traversal() {
    let fixture = CatalogFixture::new();
    fs::write(
        fixture.installation.parent().unwrap().join("outside.html"),
        "outside",
    )
    .unwrap();
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["ui"][0]["source"] = json!("../outside.html");
    fixture.write_manifest(manifest);
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn catalog_ignores_a_manifest_symlinked_outside_the_installation() {
    use std::os::unix::fs::symlink;

    let fixture = CatalogFixture::new();
    let outside = fixture.app_data.join("outside-manifest.json");
    fs::write(
        &outside,
        serde_json::to_vec(&CatalogFixture::valid_manifest()).unwrap(),
    )
    .unwrap();
    fs::remove_file(fixture.installation.join("chatos.plugin.json")).unwrap();
    symlink(&outside, fixture.installation.join("chatos.plugin.json")).unwrap();
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[tokio::test]
async fn catalog_ignores_manifests_without_windows_support() {
    let fixture = CatalogFixture::new();
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["dependencies"]["supportedPlatforms"] = json!(["macos", "linux"]);
    fixture.write_manifest(manifest);
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[tokio::test]
async fn local_http_application_requires_component_scoped_process_spawn() {
    let fixture = CatalogFixture::new();
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["permissions"] = json!([{
        "permission": "process.spawn",
        "required": true,
        "components": ["another-component"]
    }]);
    fixture.write_manifest(manifest);
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[tokio::test]
async fn catalog_admits_static_workbench_with_only_declared_ui_assets() {
    let fixture = CatalogFixture::new();
    fs::write(
        fixture.installation.join("ui/app.js"),
        "window.ready = true;",
    )
    .unwrap();
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["ui"][0].as_object_mut().unwrap().remove("runtime");
    manifest["ui"][0]["assets"] = json!(["./ui/app.js"]);
    manifest["permissions"] = json!([]);
    fixture.write_manifest(manifest);
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert_eq!(applications.len(), 1);
    assert!(!applications[0].requires_local_runtime);
}

#[tokio::test]
async fn catalog_rejects_static_assets_outside_the_declared_ui_tree() {
    let fixture = CatalogFixture::new();
    fs::write(
        fixture.installation.join("private.js"),
        "window.secret = true;",
    )
    .unwrap();
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["ui"][0].as_object_mut().unwrap().remove("runtime");
    manifest["ui"][0]["assets"] = json!(["./private.js"]);
    fixture.write_manifest(manifest);
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[tokio::test]
async fn unsafe_runtime_health_path_is_not_admitted() {
    let fixture = CatalogFixture::new();
    let mut manifest = CatalogFixture::valid_manifest();
    manifest["ui"][0]["runtime"]["healthPath"] = json!("/../admin?token=value");
    fixture.write_manifest(manifest);
    let record = fixture.record();
    let client = catalog_client(&record);
    let manager = PluginApplicationManager::new(fixture.app_data.clone());

    let applications = manager.list(&client, "owner-1").await.unwrap();

    assert!(applications.is_empty());
}

#[test]
fn blank_titles_fall_back_to_the_trimmed_interface_name() {
    let fixture = CatalogFixture::new();
    let mut manifest_value = CatalogFixture::valid_manifest();
    manifest_value["ui"][0]["title"] = json!("   ");
    manifest_value["interface"]["displayName"] = json!("  Plugin Interface  ");
    fixture.write_manifest(manifest_value);
    let record = fixture.record();
    let plugin_root = fixture
        .app_data
        .join("NativeConnector")
        .join("Plugins")
        .canonicalize()
        .unwrap();
    let (installation, manifest) = load_manifest(&plugin_root, &record).unwrap();

    let applications = project_applications(&plugin_root, &installation, &record, &manifest);

    assert_eq!(applications[0].display_name, "Plugin Interface");
}
