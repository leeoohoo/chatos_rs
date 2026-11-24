use super::*;

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

    let applications = project_applications(
        &plugin_root,
        &installation,
        &record,
        &manifest,
        None,
        &mut BTreeMap::new(),
    );

    assert_eq!(applications[0].display_name, "Plugin Interface");
}
