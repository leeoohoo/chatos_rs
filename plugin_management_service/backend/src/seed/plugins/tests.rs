// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::internal_skills::{
    internal_skill_bundle_hash, internal_skill_catalog, InternalSkillCatalogItem,
};
use super::*;
use chatos_plugin_management_sdk::verify_plugin_release_signature;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct PackagedPluginCatalog {
    schema_version: u32,
    catalog_revision: String,
    release_version: String,
    release_epoch: String,
    artifact_revision: String,
    plugins: Vec<PackagedPluginSpec>,
}

#[derive(Debug, Deserialize)]
struct PackagedPluginSpec {
    name: String,
    display_name: String,
    description: String,
    category: String,
    skill_ids: Vec<String>,
    release_version: Option<String>,
    release_epoch: Option<String>,
    artifact_revision: Option<String>,
}

#[test]
fn bundled_plugin_specs_cover_all_twenty_eight_internal_skills_once() {
    let catalog = internal_skill_catalog().expect("catalog");
    let expected = catalog
        .skills
        .iter()
        .map(|item| item.skill_id.as_str())
        .collect::<HashSet<_>>();
    let mapped = bundled_plugin_specs()
        .iter()
        .flat_map(|spec| spec.skill_ids.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(bundled_plugin_specs().len(), 12);
    assert_eq!(mapped.len(), 28);
    assert_eq!(mapped.iter().copied().collect::<HashSet<_>>(), expected);
    assert_eq!(
        bundled_plugin_specs()
            .iter()
            .find(|spec| spec.name == "figma")
            .expect("Figma spec")
            .skill_ids
            .len(),
        12
    );
    let pdf = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "pdf")
        .expect("PDF spec");
    assert_eq!(pdf.release_version, "1.22.0");
    assert_eq!(pdf.artifact_revision, "pdf-1.22.0");
    let documents = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "documents")
        .expect("Documents spec");
    assert_eq!(documents.release_version, "1.22.0");
    assert_eq!(documents.artifact_revision, "documents-1.22.0");
    let spreadsheets = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "spreadsheets")
        .expect("Spreadsheets spec");
    assert_eq!(spreadsheets.release_version, "1.8.0");
    assert_eq!(spreadsheets.artifact_revision, "spreadsheets-1.8.0");
    let presentations = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "presentations")
        .expect("Presentations spec");
    assert_eq!(presentations.release_version, "1.25.0");
    assert_eq!(presentations.artifact_revision, "presentations-1.25.0");
    let template_creator = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "template-creator")
        .expect("Template Creator spec");
    assert_eq!(template_creator.release_version, "1.2.0");
    assert_eq!(template_creator.artifact_revision, "template-creator-1.2.0");
    let browser = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "browser")
        .expect("Browser spec");
    assert_eq!(browser.release_version, "1.8.0");
    assert_eq!(browser.artifact_revision, "browser-1.8.0");
    let chrome = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "chrome")
        .expect("Chrome spec");
    assert_eq!(chrome.release_version, "1.4.0");
    assert_eq!(chrome.artifact_revision, "chrome-1.4.0");
    let computer_use = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "computer-use")
        .expect("Computer Use spec");
    assert_eq!(computer_use.release_version, "1.19.0");
    assert_eq!(computer_use.artifact_revision, "computer-use-1.19.0");
}

#[test]
fn packaged_plugin_catalog_matches_control_plane_seed_specs() {
    let packaged: PackagedPluginCatalog = serde_json::from_str(include_str!(
        "../../../../../local_connector_client/plugin_bundles/catalog/bundled-plugin-catalog.json"
    ))
    .expect("packaged Plugin catalog");
    let skill_catalog = internal_skill_catalog().expect("Skill catalog");
    assert_eq!(packaged.schema_version, 1);
    assert_eq!(packaged.catalog_revision, skill_catalog.catalog_revision);
    assert_eq!(packaged.release_version, BUNDLED_DEFAULT_RELEASE_VERSION);
    assert_eq!(packaged.release_epoch, BUNDLED_RELEASE_EPOCH);
    assert_eq!(
        packaged.artifact_revision,
        BUNDLED_INITIAL_ARTIFACT_REVISION
    );
    assert_eq!(packaged.plugins.len(), bundled_plugin_specs().len());
    for (packaged, seeded) in packaged.plugins.iter().zip(bundled_plugin_specs()) {
        assert_eq!(packaged.name, seeded.name);
        assert_eq!(packaged.display_name, seeded.display_name);
        assert_eq!(packaged.description, seeded.description);
        assert_eq!(packaged.category, seeded.category);
        assert_eq!(
            packaged
                .release_version
                .as_deref()
                .unwrap_or(BUNDLED_DEFAULT_RELEASE_VERSION),
            seeded.release_version
        );
        assert_eq!(
            packaged
                .release_epoch
                .as_deref()
                .unwrap_or(BUNDLED_RELEASE_EPOCH),
            seeded.release_epoch
        );
        assert_eq!(
            packaged
                .artifact_revision
                .as_deref()
                .unwrap_or(BUNDLED_INITIAL_ARTIFACT_REVISION),
            seeded.artifact_revision
        );
        assert_eq!(
            packaged.skill_ids,
            seeded
                .skill_ids
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn bundled_releases_have_stable_component_and_content_snapshots() {
    let catalog = internal_skill_catalog().expect("catalog");
    let records = catalog
        .skills
        .iter()
        .map(|item| (item.skill_id.clone(), skill_record(item)))
        .collect::<HashMap<_, _>>();
    validate_plugin_specs(&records).expect("complete mapping");
    let packaged_hashes = HashMap::from([
        (
            "documents",
            (
                "015bd4c3905bea73663a5eeb95d16128d78a6b2142c8fd65a842727722559c81",
                "0ec9a34cad2516c578abda5e3c8828dfed6d2dfd2347c7e60e43e368b83e09ae",
            ),
        ),
        (
            "pdf",
            (
                "014518034e5580c3a1932ac943a240ac03b96ea19b6ed7ca582d8c23e6838505",
                "67bb9c93b2c79b2eb81ded62860602b1e3dced777f54c5d4eafc17d4d4bbc370",
            ),
        ),
        (
            "spreadsheets",
            (
                "9113ea6f862664f14d8aae70f3231895874231e2c96954c77ceed15886fafd13",
                "04e33f9521146e5ae2a72e1f44baeea42117e051b0223f2e69f9e1fa4e374538",
            ),
        ),
        (
            "presentations",
            (
                "d818dbf3635d4972aa1c87316cd6b37bca98bd9c66f69a45530882c06a12531b",
                "7c485c262c17c3492bef7cec7e97bf4fe98564fd73fa716a87f67db68db968aa",
            ),
        ),
        (
            "template-creator",
            (
                "b0bfb75680c8babe9a852d1cad247726f9074c4e4966a9e21fe80ee5005aa5e3",
                "800c449df42d75fcf01cb19f962ffbcf1dcbebb8ddb00d902d90eaf02707a80a",
            ),
        ),
        (
            "remotion",
            (
                "91900e3ccfd367714323108540c165478eda351fdc56b02f22fe7c96aeb923f0",
                "d48f6e9d2ffad4ae47145fcb50397fc822e7c63b7f7c2b938f9f22a0c3b989b5",
            ),
        ),
        (
            "figma",
            (
                "8f813b9ee250424b67438fa14b3ecca4ae7d4aa317b5c03c8e03963a7db9724a",
                "eb9258ac0e4e0b563dab2852ef57e34c4a28dec5b8b1f6278e621828d9387da1",
            ),
        ),
        (
            "browser",
            (
                "28cfbb08d069dc508f9d927f3559a26c713f63adc553dcee30741e1b832789a0",
                "61193bd0762682e028366923472de877215108e4d77f66672ba38a2b39930591",
            ),
        ),
        (
            "chrome",
            (
                "dfcc537cd752da9dedf21bb12ff98cf09d7b7e04d818393526e5e42a957204df",
                "840ca2f841353ae6be6995eda720ce1639067ec95c64eec2e52e76972bcf5e12",
            ),
        ),
        (
            "computer-use",
            (
                "83f6795819b96b059314bcd0f9bcc5d89e25486638bab46c769737b8009b0b69",
                "54be7bfffc94c54a501674b360648ac8f627b6243bddc9e25492c43b6a124e6f",
            ),
        ),
        (
            "visualize",
            (
                "36aa84d2b1dd9a06dc9c8408b9dc0e72a287557e72e50778a9cdeefbeb3b3dd5",
                "e2f156a94e032e8a02f67db8907b2689472beb12cd1a5ffebc66983d7a76ea25",
            ),
        ),
        (
            "chatos-developer-kit",
            (
                "4104fb9a93de6ca64b5aa242d1e985b2a951930b5c8c05e062ecba390edcfd95",
                "6dca55f408faf1164ac7caf057ec4239ee1238240e16ad8e2d28512e590966ac",
            ),
        ),
    ]);

    for spec in bundled_plugin_specs() {
        let skills = spec
            .skill_ids
            .iter()
            .map(|skill_id| records.get(*skill_id).expect("mapped Skill").clone())
            .collect::<Vec<_>>();
        let plugin_id = bundled_plugin_id(spec.name);
        let (release, snapshots) =
            bundled_release(spec, plugin_id.as_str(), &skills).expect("bundled Release");
        let (manifest_sha256, artifact_sha256) = packaged_hashes
            .get(spec.name)
            .expect("packaged hash fixture");

        assert_eq!(release.components.len(), spec.skill_ids.len());
        assert_eq!(snapshots.len(), spec.skill_ids.len());
        assert_eq!(
            release.normalized_manifest.license.as_deref(),
            Some(PENDING_LICENSE_ID)
        );
        assert_eq!(release.artifact_sha256.len(), 64);
        assert_eq!(release.signature.manifest_sha256, *manifest_sha256);
        assert_eq!(release.artifact_sha256, *artifact_sha256);
        verify_plugin_release_signature(
            PluginReleaseVerificationContext {
                plugin_id: release.plugin_id.as_str(),
                version: release.version.as_str(),
                marketplace_id: BUNDLED_MARKETPLACE_ID,
                publisher_id: BUNDLED_PUBLISHER_ID,
                artifact_sha256: release.artifact_sha256.as_str(),
            },
            &release.normalized_manifest,
            &release.signature,
            &bundled_signing_key().expect("bundled public key"),
        )
        .expect("bundled Release signature");
        assert!(release
            .components
            .iter()
            .all(|component| component.metadata.contains_key("skill_id")));
        assert!(snapshots
            .iter()
            .all(|snapshot| snapshot.content_sha256.len() == 64));
    }
}

#[test]
fn one_bundled_plugin_can_bump_release_without_rewriting_existing_release_identity() {
    let catalog = internal_skill_catalog().expect("catalog");
    let item = catalog
        .skills
        .iter()
        .find(|item| item.skill_id == "internal_skill_documents")
        .expect("Documents Skill");
    let skills = vec![skill_record(item)];
    let existing_spec = bundled_plugin_specs()
        .iter()
        .find(|spec| spec.name == "documents")
        .expect("Documents Plugin spec");
    let plugin_id = bundled_plugin_id(existing_spec.name);
    let (existing, _) = bundled_release(existing_spec, plugin_id.as_str(), &skills)
        .expect("existing bundled Release");

    let upgraded_spec = BundledPluginSpec {
        name: existing_spec.name,
        display_name: existing_spec.display_name,
        description: existing_spec.description,
        category: existing_spec.category,
        skill_ids: existing_spec.skill_ids,
        release_version: "1.23.0",
        release_epoch: "2026-07-25T17:00:00Z",
        artifact_revision: "documents-1.23.0",
    };
    let (upgraded, upgraded_snapshots) =
        bundled_release(&upgraded_spec, plugin_id.as_str(), &skills)
            .expect("upgraded bundled Release");

    assert_eq!(existing.id, "bundled-release-documents-1-22-0");
    assert_eq!(existing.version, "1.22.0");
    assert_eq!(upgraded.id, "bundled-release-documents-1-23-0");
    assert_eq!(upgraded.version, "1.23.0");
    assert_eq!(upgraded.published_at, "2026-07-25T17:00:00Z");
    assert_ne!(upgraded.artifact_sha256, existing.artifact_sha256);
    assert!(upgraded_snapshots
        .iter()
        .all(|snapshot| snapshot.release_id == upgraded.id));
}

fn skill_record(item: &InternalSkillCatalogItem) -> SkillRecord {
    let mut metadata = ResourceMetadata::default();
    metadata.extra.insert(
        "implementation_status".to_string(),
        serde_json::json!(item.implementation_status),
    );
    metadata.extra.insert(
        "permissions".to_string(),
        serde_json::json!(item.permissions),
    );
    SkillRecord {
        id: item.skill_id.clone(),
        owner_user_id: "admin".to_string(),
        owner_kind: OWNER_KIND_ADMIN.to_string(),
        visibility: VISIBILITY_SYSTEM_PRIVATE.to_string(),
        source_kind: SOURCE_KIND_ADMIN_CREATED.to_string(),
        name: item.name.clone(),
        display_name: item.display_name.clone(),
        description: Some(item.description.clone()),
        enabled: true,
        content: SkillContent {
            kind: SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE.to_string(),
            bundle_id: Some(item.bundle_id.clone()),
            bundle_version: Some(item.version.clone()),
            bundle_hash: Some(internal_skill_bundle_hash(item)),
            entrypoint_kind: Some(item.entrypoint_kind.clone()),
            ..SkillContent::default()
        },
        metadata,
        plugin_component: PluginComponentOwnership::default(),
        created_by: "admin".to_string(),
        updated_by: "admin".to_string(),
        created_at: BUNDLED_RELEASE_EPOCH.to_string(),
        updated_at: BUNDLED_RELEASE_EPOCH.to_string(),
    }
}
