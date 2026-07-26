// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, plugin_component_descriptors,
    plugin_release_signing_payload, validate_plugin_manifest, PluginAuthor, PluginDependencySpec,
    PluginPathRef, PluginPermissionRequirement, PluginReleaseVerificationContext,
    PLUGIN_MANIFEST_SCHEMA_VERSION_V1, PLUGIN_SIGNATURE_ALGORITHM_ED25519,
    PLUGIN_SIGNING_KEY_USAGE_RELEASE,
};
use chrono::DateTime;
use ring::signature::{Ed25519KeyPair, KeyPair};
use semver::Version;
use sha2::{Digest, Sha256};

use super::internal_skills::{internal_skill_catalog, InternalSkillCatalog};
use super::*;

mod specs;

use specs::*;

const BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";
const BUNDLED_PUBLISHER_ID: &str = "chatos";
const BUNDLED_KEY_ID: &str = "chatos-bundled-attestation-v1";
const BUNDLED_SIGNING_SEED_CONTEXT: &[u8] = b"chatos-bundled-attestation-seed-v1";
const PENDING_LICENSE_ID: &str = "LicenseRef-Pending-Redistribution-Review";

pub(super) async fn seed_bundled_plugins(store: &AppStore) -> Result<(), String> {
    let skill_catalog = internal_skill_catalog()?;
    let skills = load_seeded_skills(store, &skill_catalog).await?;
    validate_plugin_specs(&skills)?;
    seed_bundled_marketplace(store, skill_catalog.catalog_revision.as_str()).await?;
    for spec in bundled_plugin_specs() {
        seed_bundled_plugin(store, &skills, spec).await?;
    }
    Ok(())
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
    let Some(existing) = store.get_plugin_release(release.id.as_str()).await? else {
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

fn bundled_release_signature(
    plugin_id: &str,
    version: &str,
    manifest_sha256: String,
    artifact_sha256: &str,
    signed_at: &str,
) -> Result<PluginReleaseSignature, String> {
    let keypair = bundled_signing_keypair()?;
    let mut signature = PluginReleaseSignature {
        key_id: BUNDLED_KEY_ID.to_string(),
        publisher_id: BUNDLED_PUBLISHER_ID.to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        marketplace_id: BUNDLED_MARKETPLACE_ID.to_string(),
        signature_base64: String::new(),
        signed_at: signed_at.to_string(),
        manifest_sha256,
    };
    let payload = plugin_release_signing_payload(
        PluginReleaseVerificationContext {
            plugin_id,
            version,
            marketplace_id: BUNDLED_MARKETPLACE_ID,
            publisher_id: BUNDLED_PUBLISHER_ID,
            artifact_sha256,
        },
        &signature,
    )
    .map_err(|err| err.to_string())?;
    signature.signature_base64 = STANDARD.encode(keypair.sign(payload.as_slice()).as_ref());
    Ok(signature)
}

fn bundled_signing_key() -> Result<SigningKeyRef, String> {
    let keypair = bundled_signing_keypair()?;
    Ok(SigningKeyRef {
        key_id: BUNDLED_KEY_ID.to_string(),
        publisher_id: BUNDLED_PUBLISHER_ID.to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        public_key_base64: STANDARD.encode(keypair.public_key().as_ref()),
        usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
        valid_from: BUNDLED_RELEASE_EPOCH.to_string(),
        valid_until: None,
        revoked_at: None,
    })
}

fn bundled_signing_keypair() -> Result<Ed25519KeyPair, String> {
    // This deterministic key only attests compile-time bundled content. Network artifacts are
    // never allowed to inherit the bundled marketplace trust scope.
    let seed = Sha256::digest(BUNDLED_SIGNING_SEED_CONTEXT);
    Ed25519KeyPair::from_seed_unchecked(seed.as_slice())
        .map_err(|_| "construct deterministic bundled Plugin attestation key failed".to_string())
}

fn bundled_plugin_id(name: &str) -> String {
    format!("bundled-plugin-{name}")
}

fn bundled_release_id(name: &str, version: &str) -> String {
    let version = version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("bundled-release-{name}-{version}")
}

#[cfg(test)]
mod tests;
