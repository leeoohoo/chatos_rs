// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, parse_plugin_manifest, plugin_component_descriptors,
    plugin_release_signing_payload, validate_plugin_manifest, PluginAuthor, PluginDependencySpec,
    PluginManifestSource, PluginPathRef, PluginPermissionRequirement,
    PluginReleaseVerificationContext, SystemAgentKey, PLUGIN_MANIFEST_SCHEMA_VERSION_V1,
    PLUGIN_SIGNATURE_ALGORITHM_ED25519, PLUGIN_SIGNING_KEY_USAGE_RELEASE,
};
use chatos_plugin_package::{
    build_portable_component_bundles, verify_embedded_plugin_package_files, PluginPackageLimits,
};
use chrono::DateTime;
use ring::signature::{Ed25519KeyPair, KeyPair};
use semver::Version;
use sha2::{Digest, Sha256};

use super::internal_skills::{internal_skill_catalog, InternalSkillCatalog};
use super::*;

mod specs;

use specs::*;

mod signing;
use signing::*;

const BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";
const BUNDLED_PUBLISHER_ID: &str = "chatos";
const BUNDLED_KEY_ID: &str = "chatos-bundled-attestation-v1";
const BUNDLED_SIGNING_SEED_CONTEXT: &[u8] = b"chatos-bundled-attestation-seed-v1";
const PENDING_LICENSE_ID: &str = "LicenseRef-Pending-Redistribution-Review";
const BUNDLED_MARKETPLACE_REVISION: &str = "2026-08-01.1";
pub(super) const BUNDLED_PONYTAIL_PLUGIN_ID: &str = "bundled-plugin-ponytail";
pub(super) const BUNDLED_PONYTAIL_AGENT_KEYS: [&str; 1] =
    [SystemAgentKey::TaskRunnerRunPhase.as_str()];
const BUNDLED_PONYTAIL_VERSION: &str = "4.8.4-chatos.3";
const BUNDLED_PONYTAIL_RELEASE_EPOCH: &str = "2026-08-01T00:00:00Z";
const BUNDLED_PONYTAIL_ARTIFACT_SHA256: &str =
    "77df53d6f84bb54c5fd97159fa3847b5571409f2bec90bbe80b26c7a03d0718b";

pub(super) async fn seed_bundled_plugins(
    store: &AppStore,
    admin_user_id: &str,
) -> Result<(), String> {
    let skill_catalog = internal_skill_catalog()?;
    let skills = load_seeded_skills(store, &skill_catalog).await?;
    validate_plugin_specs(&skills)?;
    seed_bundled_marketplace(store, BUNDLED_MARKETPLACE_REVISION).await?;
    for spec in bundled_plugin_specs() {
        seed_bundled_plugin(store, &skills, spec).await?;
    }
    seed_bundled_ponytail(store, admin_user_id).await?;
    Ok(())
}

async fn seed_bundled_ponytail(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    if let Some(existing) = store
        .find_plugin_catalog_entry(BUNDLED_MARKETPLACE_ID, "ponytail")
        .await?
    {
        if existing.id != BUNDLED_PONYTAIL_PLUGIN_ID {
            return Err("bundled Ponytail Plugin identity is already claimed".to_string());
        }
    }
    let (release, snapshots, mut catalog) = bundled_ponytail_release()?;
    store
        .set_plugin_release_publication_ready(release.id.as_str(), false)
        .await?;
    let release = persist_immutable_release(store, release).await?;
    store
        .replace_plugin_component_snapshots(
            BUNDLED_PONYTAIL_PLUGIN_ID,
            release.id.as_str(),
            snapshots.as_slice(),
        )
        .await?;
    store
        .set_plugin_release_publication_ready(release.id.as_str(), true)
        .await?;

    if let Some(existing) = store
        .get_plugin_catalog_entry(BUNDLED_PONYTAIL_PLUGIN_ID)
        .await?
    {
        catalog.enabled = existing.enabled;
        catalog.featured = existing.featured;
        catalog.created_at = existing.created_at.clone();
        catalog.updated_at = existing.updated_at.clone();
        if catalog != existing {
            catalog.updated_at = now_rfc3339();
        }
    }
    store.replace_plugin_catalog_entry(&catalog).await?;

    if store
        .get_user_plugin_preference(admin_user_id, BUNDLED_PONYTAIL_PLUGIN_ID)
        .await?
        .is_none()
    {
        store
            .replace_user_plugin_preference(&UserPluginPreferenceRecord {
                owner_user_id: admin_user_id.to_string(),
                plugin_id: BUNDLED_PONYTAIL_PLUGIN_ID.to_string(),
                enabled: true,
                auto_update: true,
                release_channel: "stable".to_string(),
                enabled_components: Vec::new(),
                updated_at: BUNDLED_PONYTAIL_RELEASE_EPOCH.to_string(),
            })
            .await?;
    }
    Ok(())
}

fn bundled_ponytail_release() -> Result<
    (
        PluginReleaseRecord,
        Vec<PluginComponentSnapshot>,
        PluginCatalogRecord,
    ),
    String,
> {
    let manifest = parse_plugin_manifest(
        include_str!("bundled_plugins/ponytail/.chatos-plugin/plugin.json"),
        PluginManifestSource::Chatos,
    )
    .map_err(|error| error.to_string())?;
    if manifest.name != "ponytail" || manifest.version != BUNDLED_PONYTAIL_VERSION {
        return Err("bundled Ponytail Manifest identity drift detected".to_string());
    }
    let components = plugin_component_descriptors(&manifest);
    let manifest_sha256 =
        normalized_plugin_manifest_sha256(&manifest).map_err(|error| error.to_string())?;
    let release_id = bundled_release_id("ponytail", BUNDLED_PONYTAIL_VERSION);
    let signature = bundled_release_signature(
        BUNDLED_PONYTAIL_PLUGIN_ID,
        BUNDLED_PONYTAIL_VERSION,
        manifest_sha256,
        BUNDLED_PONYTAIL_ARTIFACT_SHA256,
        BUNDLED_PONYTAIL_RELEASE_EPOCH,
    )?;
    let release = PluginReleaseRecord {
        id: release_id,
        plugin_id: BUNDLED_PONYTAIL_PLUGIN_ID.to_string(),
        version: BUNDLED_PONYTAIL_VERSION.to_string(),
        manifest_schema_version: manifest.schema_version,
        normalized_manifest: manifest.clone(),
        artifact_ref: format!("bundled://plugins/ponytail/{BUNDLED_PONYTAIL_VERSION}.zip"),
        artifact_sha256: BUNDLED_PONYTAIL_ARTIFACT_SHA256.to_string(),
        signature,
        sbom_ref: Some("./sbom.spdx.json".to_string()),
        supported_platforms: Vec::new(),
        components,
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        release_channel: "stable".to_string(),
        published_at: BUNDLED_PONYTAIL_RELEASE_EPOCH.to_string(),
        revoked_at: None,
    };
    let package = verify_embedded_plugin_package_files(
        embedded_ponytail_files(),
        &release,
        PluginPackageLimits {
            max_archive_bytes: 16 * 1024 * 1024,
            max_entries: 512,
            max_file_bytes: 2 * 1024 * 1024,
            max_unpacked_bytes: 32 * 1024 * 1024,
            ..PluginPackageLimits::default()
        },
    )
    .map_err(|error| format!("verify bundled Ponytail package failed: {error}"))?;
    let bundles =
        build_portable_component_bundles(&release, &package, BUNDLED_PONYTAIL_RELEASE_EPOCH)
            .map_err(|error| format!("build bundled Ponytail portable Bundles failed: {error}"))?;
    let bundle_hashes = bundles
        .iter()
        .map(|bundle| (bundle.component_key.as_str(), bundle.bundle_sha256.as_str()))
        .collect::<HashMap<_, _>>();
    let snapshots = release
        .components
        .iter()
        .map(|component| {
            let content_sha256 = bundle_hashes
                .get(component.component_key.as_str())
                .ok_or_else(|| {
                    format!(
                        "bundled Ponytail component has no canonical Bundle: {}",
                        component.component_key
                    )
                })?;
            Ok(PluginComponentSnapshot {
                plugin_id: release.plugin_id.clone(),
                release_id: release.id.clone(),
                component: component.clone(),
                content_sha256: (*content_sha256).to_string(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let catalog = PluginCatalogRecord {
        id: BUNDLED_PONYTAIL_PLUGIN_ID.to_string(),
        plugin_key: format!("ponytail@{BUNDLED_MARKETPLACE_ID}"),
        marketplace_id: BUNDLED_MARKETPLACE_ID.to_string(),
        owner_user_id: None,
        name: manifest.name.clone(),
        display_name: manifest.interface.display_name.clone(),
        description: manifest.description.clone(),
        publisher: PluginPublisher {
            id: BUNDLED_PUBLISHER_ID.to_string(),
            name: "ChatOS".to_string(),
            website: manifest.repository.clone(),
            verified: true,
        },
        interface: manifest.interface.clone(),
        keywords: manifest.keywords.clone(),
        visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
        featured: true,
        enabled: true,
        latest_release_id: release.id.clone(),
        license: PluginLicenseMetadata {
            license_id: "MIT".to_string(),
            license_url: Some(
                "https://github.com/DietrichGebert/ponytail/blob/main/LICENSE".to_string(),
            ),
            redistributable: true,
            reviewed_at: Some(BUNDLED_PONYTAIL_RELEASE_EPOCH.to_string()),
        },
        created_at: BUNDLED_PONYTAIL_RELEASE_EPOCH.to_string(),
        updated_at: BUNDLED_PONYTAIL_RELEASE_EPOCH.to_string(),
    };
    Ok((release, snapshots, catalog))
}

fn embedded_ponytail_files() -> BTreeMap<String, Vec<u8>> {
    macro_rules! embedded {
        ($path:literal) => {
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/seed/bundled_plugins/ponytail/",
                $path
            ))
            .to_vec()
        };
    }
    BTreeMap::from([
        (
            ".chatos-plugin/checksums.json".to_string(),
            embedded!(".chatos-plugin/checksums.json"),
        ),
        (
            ".chatos-plugin/plugin.json".to_string(),
            embedded!(".chatos-plugin/plugin.json"),
        ),
        (
            "CHATOS-ADAPTATION.md".to_string(),
            embedded!("CHATOS-ADAPTATION.md"),
        ),
        (
            "agents/ponytail-full-local.md".to_string(),
            embedded!("agents/ponytail-full-local.md"),
        ),
        (
            "agents/ponytail-lite-local.md".to_string(),
            embedded!("agents/ponytail-lite-local.md"),
        ),
        (
            "agents/ponytail-ultra-local.md".to_string(),
            embedded!("agents/ponytail-ultra-local.md"),
        ),
        ("assets/logo.svg".to_string(), embedded!("assets/logo.svg")),
        (
            "commands/ponytail-audit.md".to_string(),
            embedded!("commands/ponytail-audit.md"),
        ),
        (
            "commands/ponytail-debt.md".to_string(),
            embedded!("commands/ponytail-debt.md"),
        ),
        (
            "commands/ponytail-help.md".to_string(),
            embedded!("commands/ponytail-help.md"),
        ),
        (
            "commands/ponytail-review.md".to_string(),
            embedded!("commands/ponytail-review.md"),
        ),
        (
            "licenses/ponytail-LICENSE".to_string(),
            embedded!("licenses/ponytail-LICENSE"),
        ),
        ("sbom.spdx.json".to_string(), embedded!("sbom.spdx.json")),
        (
            "skills/ponytail/SKILL.md".to_string(),
            embedded!("skills/ponytail/SKILL.md"),
        ),
    ])
}

async fn load_seeded_skills(
    store: &AppStore,
    catalog: &InternalSkillCatalog,
) -> Result<HashMap<String, SkillRecord>, String> {
    let mut records = HashMap::new();
    for item in &catalog.skills {
        let record = store
            .get_skill(item.skill_id.as_str())
            .await?
            .ok_or_else(|| format!("seeded internal Skill is missing: {}", item.skill_id))?;
        records.insert(record.id.clone(), record);
    }
    Ok(records)
}

async fn seed_bundled_marketplace(store: &AppStore, revision: &str) -> Result<(), String> {
    if let Some(existing) = store
        .find_plugin_marketplace_by_name(BUNDLED_MARKETPLACE_ID)
        .await?
    {
        if existing.id != BUNDLED_MARKETPLACE_ID {
            return Err("bundled Plugin marketplace name is already claimed".to_string());
        }
    }
    let existing = store.get_plugin_marketplace(BUNDLED_MARKETPLACE_ID).await?;
    let last_synced_at = existing
        .as_ref()
        .filter(|record| record.last_catalog_revision.as_deref() == Some(revision))
        .and_then(|record| record.last_synced_at.clone())
        .unwrap_or_else(now_rfc3339);
    let record = PluginMarketplaceRecord {
        id: BUNDLED_MARKETPLACE_ID.to_string(),
        name: BUNDLED_MARKETPLACE_ID.to_string(),
        owner_user_id: None,
        visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
        source_kind: PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY.to_string(),
        catalog_url: None,
        enabled: existing
            .as_ref()
            .map(|record| record.enabled)
            .unwrap_or(true),
        trust_level: PLUGIN_TRUST_BUNDLED.to_string(),
        trusted_signing_keys: vec![bundled_signing_key()?],
        last_catalog_revision: Some(revision.to_string()),
        last_synced_at: Some(last_synced_at),
    };
    store.replace_plugin_marketplace(&record).await
}

async fn seed_bundled_plugin(
    store: &AppStore,
    skills: &HashMap<String, SkillRecord>,
    spec: &BundledPluginSpec,
) -> Result<(), String> {
    let plugin_id = bundled_plugin_id(spec.name);
    if let Some(existing) = store
        .find_plugin_catalog_entry(BUNDLED_MARKETPLACE_ID, spec.name)
        .await?
    {
        if existing.id != plugin_id {
            return Err(format!(
                "bundled Plugin identity is already claimed: {}",
                spec.name
            ));
        }
    }
    let selected = spec
        .skill_ids
        .iter()
        .map(|skill_id| {
            skills
                .get(*skill_id)
                .cloned()
                .ok_or_else(|| format!("bundled Plugin Skill is missing: {skill_id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (release, snapshots) = bundled_release(spec, &plugin_id, &selected)?;
    let release = persist_immutable_release(store, release).await?;
    store
        .replace_plugin_component_snapshots(&plugin_id, &release.id, &snapshots)
        .await?;

    let existing = store.get_plugin_catalog_entry(plugin_id.as_str()).await?;
    let now = now_rfc3339();
    let mut catalog = bundled_catalog_record(
        spec,
        &plugin_id,
        if release.revoked_at.is_none() {
            release.id.as_str()
        } else {
            ""
        },
        release.normalized_manifest.interface.clone(),
        spec.release_epoch,
    );
    if let Some(existing) = existing {
        catalog.enabled = existing.enabled;
        catalog.featured = existing.featured;
        catalog.created_at = existing.created_at.clone();
        catalog.updated_at = existing.updated_at.clone();
        if catalog != existing {
            catalog.updated_at = now;
        }
    }
    store.replace_plugin_catalog_entry(&catalog).await
}

async fn persist_immutable_release(
    store: &AppStore,
    release: PluginReleaseRecord,
) -> Result<PluginReleaseRecord, String> {
    let Some(existing) = store
        .get_plugin_release_any_state(release.id.as_str())
        .await?
    else {
        store.insert_plugin_release(&release).await?;
        return Ok(release);
    };
    let mut expected = release;
    expected.revoked_at = existing.revoked_at.clone();
    if existing == expected {
        Ok(existing)
    } else {
        Err(format!(
            "bundled Plugin Release drift detected for immutable release {}",
            existing.id
        ))
    }
}

fn bundled_release(
    spec: &BundledPluginSpec,
    plugin_id: &str,
    skills: &[SkillRecord],
) -> Result<(PluginReleaseRecord, Vec<PluginComponentSnapshot>), String> {
    let release_version = Version::parse(spec.release_version).map_err(|error| {
        format!(
            "bundled Plugin {} has invalid Release version {}: {error}",
            spec.name, spec.release_version
        )
    })?;
    if !release_version.pre.is_empty() || !release_version.build.is_empty() {
        return Err(format!(
            "bundled Plugin {} Release version must be stable x.y.z: {}",
            spec.name, spec.release_version
        ));
    }
    let release_epoch = DateTime::parse_from_rfc3339(spec.release_epoch).map_err(|error| {
        format!(
            "bundled Plugin {} has invalid Release epoch {}: {error}",
            spec.name, spec.release_epoch
        )
    })?;
    if release_epoch.offset().local_minus_utc() != 0 || !spec.release_epoch.ends_with('Z') {
        return Err(format!(
            "bundled Plugin {} Release epoch must use UTC Z notation",
            spec.name
        ));
    }
    if spec.artifact_revision.is_empty()
        || spec.artifact_revision.len() > 120
        || !spec
            .artifact_revision
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !spec
            .artifact_revision
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "bundled Plugin {} has invalid artifact revision",
            spec.name
        ));
    }
    let release_id = bundled_release_id(spec.name, spec.release_version);
    let permissions = bundled_permissions(skills);
    let manifest = PluginManifest {
        schema_version: PLUGIN_MANIFEST_SCHEMA_VERSION_V1,
        execution: Default::default(),
        name: spec.name.to_string(),
        version: spec.release_version.to_string(),
        description: spec.description.to_string(),
        author: PluginAuthor {
            name: "ChatOS".to_string(),
            email: None,
            url: None,
        },
        homepage: None,
        repository: None,
        license: Some(PENDING_LICENSE_ID.to_string()),
        keywords: vec!["bundled".to_string(), "skills".to_string()],
        skills: skills
            .iter()
            .map(|skill| PluginPathRef::new(format!("./skills/{}", skill.name)))
            .collect(),
        mcp_servers: Vec::new(),
        apps: Vec::new(),
        commands: Vec::new(),
        agents: Vec::new(),
        hooks: Vec::new(),
        ui: Vec::new(),
        interface: bundled_interface(spec, skills),
        dependencies: PluginDependencySpec::default(),
        permissions: permissions.clone(),
        bundled_content_variant: Some("chatos-internal-skill-bundles-v2".to_string()),
    };
    validate_plugin_manifest(&manifest).map_err(|err| err.to_string())?;
    let mut components = plugin_component_descriptors(&manifest);
    let skill_by_name = skills
        .iter()
        .map(|skill| (skill.name.as_str(), skill))
        .collect::<HashMap<_, _>>();
    let mut snapshots = Vec::with_capacity(components.len());
    for component in &mut components {
        let skill = skill_by_name
            .get(component.component_key.as_str())
            .ok_or_else(|| {
                format!(
                    "bundled Plugin component does not map to a Skill: {}",
                    component.component_key
                )
            })?;
        let bundle_hash = required_skill_content(skill, "bundle_hash", |content| {
            content.bundle_hash.as_deref()
        })?;
        let bundle_id =
            required_skill_content(skill, "bundle_id", |content| content.bundle_id.as_deref())?;
        let entrypoint_kind = required_skill_content(skill, "entrypoint_kind", |content| {
            content.entrypoint_kind.as_deref()
        })?;
        component.display_name = skill.display_name.clone();
        component.runtime_kind = entrypoint_kind.to_string();
        component
            .metadata
            .insert("skill_id".to_string(), serde_json::json!(skill.id));
        component
            .metadata
            .insert("bundle_id".to_string(), serde_json::json!(bundle_id));
        component
            .metadata
            .insert("bundle_hash".to_string(), serde_json::json!(bundle_hash));
        component.metadata.insert(
            "implementation_status".to_string(),
            skill
                .metadata
                .extra
                .get("implementation_status")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        );
        snapshots.push(PluginComponentSnapshot {
            plugin_id: plugin_id.to_string(),
            release_id: release_id.clone(),
            component: component.clone(),
            content_sha256: bundle_hash.to_string(),
        });
    }
    let manifest_sha256 =
        normalized_plugin_manifest_sha256(&manifest).map_err(|err| err.to_string())?;
    let artifact_sha256 = bundled_artifact_sha256(spec.name, spec.artifact_revision, skills)?;
    let signature = bundled_release_signature(
        plugin_id,
        manifest.version.as_str(),
        manifest_sha256,
        artifact_sha256.as_str(),
        spec.release_epoch,
    )?;
    Ok((
        PluginReleaseRecord {
            id: release_id,
            plugin_id: plugin_id.to_string(),
            version: spec.release_version.to_string(),
            manifest_schema_version: manifest.schema_version,
            normalized_manifest: manifest,
            artifact_ref: format!(
                "bundled://internal-skills/{}/{}",
                spec.name, spec.release_version
            ),
            artifact_sha256,
            signature,
            sbom_ref: None,
            supported_platforms: Vec::new(),
            components,
            dependencies: PluginDependencySpec::default(),
            permissions,
            release_channel: "stable".to_string(),
            published_at: spec.release_epoch.to_string(),
            revoked_at: None,
        },
        snapshots,
    ))
}

fn bundled_catalog_record(
    spec: &BundledPluginSpec,
    plugin_id: &str,
    release_id: &str,
    interface: PluginInterfaceMetadata,
    timestamp: &str,
) -> PluginCatalogRecord {
    PluginCatalogRecord {
        id: plugin_id.to_string(),
        plugin_key: format!("{}@{}", spec.name, BUNDLED_MARKETPLACE_ID),
        marketplace_id: BUNDLED_MARKETPLACE_ID.to_string(),
        owner_user_id: None,
        name: spec.name.to_string(),
        display_name: spec.display_name.to_string(),
        description: spec.description.to_string(),
        publisher: PluginPublisher {
            id: BUNDLED_PUBLISHER_ID.to_string(),
            name: "ChatOS".to_string(),
            website: None,
            verified: true,
        },
        interface,
        keywords: vec!["bundled".to_string(), "skills".to_string()],
        visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
        featured: false,
        enabled: true,
        latest_release_id: release_id.to_string(),
        license: PluginLicenseMetadata {
            license_id: PENDING_LICENSE_ID.to_string(),
            license_url: None,
            redistributable: false,
            reviewed_at: None,
        },
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    }
}

fn bundled_interface(spec: &BundledPluginSpec, skills: &[SkillRecord]) -> PluginInterfaceMetadata {
    let mut capabilities = skills
        .iter()
        .filter_map(|skill| skill.content.entrypoint_kind.clone())
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    if capabilities.is_empty() {
        capabilities.push("skills".to_string());
    }
    PluginInterfaceMetadata {
        display_name: spec.display_name.to_string(),
        short_description: spec.description.to_string(),
        long_description: spec.description.to_string(),
        developer_name: "ChatOS".to_string(),
        category: spec.category.to_string(),
        capabilities,
        website_url: None,
        privacy_policy_url: None,
        terms_of_service_url: None,
        default_prompt: Vec::new(),
        brand_color: None,
        composer_icon: None,
        logo: None,
        logo_dark: None,
        screenshots: Vec::new(),
    }
}

fn bundled_permissions(skills: &[SkillRecord]) -> Vec<PluginPermissionRequirement> {
    let mut by_permission = BTreeMap::<String, Vec<String>>::new();
    for skill in skills {
        let permissions = skill
            .metadata
            .extra
            .get("permissions")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str);
        for permission in permissions {
            by_permission
                .entry(permission.to_string())
                .or_default()
                .push(skill.name.clone());
        }
    }
    by_permission
        .into_iter()
        .map(|(permission, mut components)| {
            components.sort();
            components.dedup();
            PluginPermissionRequirement {
                permission,
                required: true,
                reason: Some("Required by bundled Skill components".to_string()),
                components,
            }
        })
        .collect()
}

fn bundled_artifact_sha256(
    plugin_name: &str,
    artifact_revision: &str,
    skills: &[SkillRecord],
) -> Result<String, String> {
    let mut parts = skills
        .iter()
        .map(|skill| {
            required_skill_content(skill, "bundle_hash", |content| {
                content.bundle_hash.as_deref()
            })
            .map(|bundle_hash| format!("{}:{bundle_hash}", skill.id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    parts.sort();
    let payload = format!(
        "chatos-bundled-plugin-release-v1\n{plugin_name}\n{artifact_revision}\n{}",
        parts.join("\n")
    );
    Ok(hex::encode(Sha256::digest(payload.as_bytes())))
}

fn required_skill_content<'a>(
    skill: &'a SkillRecord,
    field: &str,
    select: impl FnOnce(&'a SkillContent) -> Option<&'a str>,
) -> Result<&'a str, String> {
    select(&skill.content).ok_or_else(|| {
        format!(
            "bundled internal Skill {} is missing content.{field}",
            skill.id
        )
    })
}

fn validate_plugin_specs(skills: &HashMap<String, SkillRecord>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for spec in bundled_plugin_specs() {
        for skill_id in spec.skill_ids {
            if !skills.contains_key(*skill_id) {
                return Err(format!("bundled Plugin maps unknown Skill: {skill_id}"));
            }
            if !seen.insert(*skill_id) {
                return Err(format!(
                    "bundled Skill is mapped more than once: {skill_id}"
                ));
            }
        }
    }
    if seen.len() != skills.len() {
        let mut missing = skills
            .keys()
            .filter(|skill_id| !seen.contains(skill_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        missing.sort();
        return Err(format!(
            "internal Skills missing bundled Plugin mapping: {}",
            missing.join(", ")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
