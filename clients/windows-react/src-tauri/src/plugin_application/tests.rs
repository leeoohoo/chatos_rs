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
                "components": ["plugin-workbench"]
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
