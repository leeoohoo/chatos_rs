// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::models::*;
use crate::store::{now_rfc3339, AppStore};

#[derive(Debug, Deserialize)]
pub(super) struct InternalSkillCatalog {
    pub(super) catalog_revision: String,
    pub(super) skills: Vec<InternalSkillCatalogItem>,
}

#[derive(Debug, Deserialize)]
pub(super) struct InternalSkillCatalogItem {
    pub(super) skill_id: String,
    bundle_id: String,
    version: String,
    name: String,
    display_name: String,
    description: String,
    category: String,
    entrypoint_kind: String,
    implementation_status: String,
    #[serde(default)]
    requires_workspace: bool,
    #[serde(default)]
    permissions: Vec<String>,
}

pub(super) fn internal_skill_catalog() -> Result<InternalSkillCatalog, String> {
    serde_json::from_str(include_str!(
        "../../../../local_connector_client/skill_bundles/catalog/internal-skill-catalog.json"
    ))
    .map_err(|err| format!("decode internal Skill catalog failed: {err}"))
}

pub(super) async fn seed_internal_skills(
    store: &AppStore,
    admin_user_id: &str,
) -> Result<(), String> {
    let catalog = internal_skill_catalog()?;
    for item in catalog.skills {
        let bundle_hash = internal_skill_bundle_hash(&item);
        let existing = store.get_skill(item.skill_id.as_str()).await?;
        let now = now_rfc3339();
        let mut metadata = existing
            .as_ref()
            .map(|record| record.metadata.clone())
            .unwrap_or_default();
        metadata.version = Some(item.version.clone());
        metadata.category = Some(item.category.clone());
        metadata.tags = vec![
            "internal".to_string(),
            "bundled".to_string(),
            item.category.clone(),
        ];
        metadata.extra.insert(
            "catalog_revision".to_string(),
            serde_json::json!(catalog.catalog_revision.clone()),
        );
        metadata.extra.insert(
            "implementation_status".to_string(),
            serde_json::json!(item.implementation_status.clone()),
        );
        metadata.extra.insert(
            "requires_workspace".to_string(),
            serde_json::json!(item.requires_workspace),
        );
        metadata.extra.insert(
            "permissions".to_string(),
            serde_json::json!(item.permissions.clone()),
        );
        let record = SkillRecord {
            id: item.skill_id,
            owner_user_id: admin_user_id.to_string(),
            owner_kind: OWNER_KIND_ADMIN.to_string(),
            visibility: VISIBILITY_SYSTEM_PRIVATE.to_string(),
            source_kind: SOURCE_KIND_ADMIN_CREATED.to_string(),
            name: item.name,
            display_name: item.display_name,
            description: Some(item.description),
            enabled: existing
                .as_ref()
                .map(|record| record.enabled)
                .unwrap_or(true),
            content: SkillContent {
                kind: SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE.to_string(),
                bundle_hash: Some(bundle_hash),
                bundle_id: Some(item.bundle_id),
                bundle_version: Some(item.version),
                entrypoint_kind: Some(item.entrypoint_kind),
                ..SkillContent::default()
            },
            metadata,
            created_by: existing
                .as_ref()
                .map(|record| record.created_by.clone())
                .unwrap_or_else(|| admin_user_id.to_string()),
            updated_by: admin_user_id.to_string(),
            created_at: existing
                .as_ref()
                .map(|record| record.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        store.replace_skill(&record).await?;
    }
    Ok(())
}

fn internal_skill_bundle_hash(item: &InternalSkillCatalogItem) -> String {
    let instructions_hash = internal_skill_instructions(item.skill_id.as_str())
        .map(|value| hex::encode(Sha256::digest(value.as_bytes())))
        .unwrap_or_else(|| "none".to_string());
    let manifest_hash = internal_skill_manifest(item.skill_id.as_str())
        .map(|value| hex::encode(Sha256::digest(value.as_bytes())))
        .unwrap_or_else(|| "none".to_string());
    let payload = format!(
        "chatos-internal-skill-bundle-v2\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        item.skill_id,
        item.bundle_id,
        item.version,
        item.entrypoint_kind,
        item.implementation_status,
        instructions_hash,
        manifest_hash,
        item.requires_workspace,
        item.permissions.join(","),
    );
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn internal_skill_manifest(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "internal_skill_plugin_creator" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/plugin-creator/1.0.0/skill.json"
        )),
        "internal_skill_openai_docs" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/openai-docs/1.0.0/skill.json"
        )),
        "internal_skill_skill_creator" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/skill-creator/1.0.0/skill.json"
        )),
        "internal_skill_skill_installer" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/skill-installer/1.0.0/skill.json"
        )),
        "internal_skill_remotion" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/remotion-best-practices/1.0.0/skill.json"
        )),
        "internal_skill_visualize" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/visualize/1.0.0/skill.json"
        )),
        "internal_skill_documents" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/documents/1.0.0/skill.json"
        )),
        "internal_skill_pdf" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/pdf/1.0.0/skill.json"
        )),
        "internal_skill_presentations" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/presentations/1.0.0/skill.json"
        )),
        "internal_skill_spreadsheets" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/spreadsheets/1.0.0/skill.json"
        )),
        "internal_skill_template_creator" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/template-creator/1.0.0/skill.json"
        )),
        "internal_skill_imagegen" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/imagegen/1.0.0/skill.json"
        )),
        "internal_skill_figma_code_connect" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-code-connect/1.0.0/skill.json"
        )),
        "internal_skill_figma_create_new_file" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-create-new-file/1.0.0/skill.json"
        )),
        "internal_skill_figma_design_to_code" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-design-to-code/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_design" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-generate-design/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_diagram" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-generate-diagram/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_library" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-generate-library/1.0.0/skill.json"
        )),
        "internal_skill_figma_implement_motion" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-implement-motion/1.0.0/skill.json"
        )),
        "internal_skill_figma_swiftui" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-swiftui/1.0.0/skill.json"
        )),
        "internal_skill_figma_use" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_figjam" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use-figjam/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_motion" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use-motion/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_slides" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use-slides/1.0.0/skill.json"
        )),
        "internal_skill_browser" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/control-in-app-browser/1.0.0/skill.json"
        )),
        "internal_skill_computer_use" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/computer-use/1.0.0/skill.json"
        )),
        "internal_skill_excel_live_control" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/excel-live-control/1.0.0/skill.json"
        )),
        _ => None,
    }
}

fn internal_skill_instructions(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "internal_skill_plugin_creator" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/plugin-creator/1.0.0/instructions.md"
        )),
        "internal_skill_openai_docs" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/openai-docs/1.0.0/instructions.md"
        )),
        "internal_skill_skill_creator" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/skill-creator/1.0.0/instructions.md"
        )),
        "internal_skill_skill_installer" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/skill-installer/1.0.0/instructions.md"
        )),
        "internal_skill_remotion" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/remotion-best-practices/1.0.0/instructions.md"
        )),
        "internal_skill_visualize" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/visualize/1.0.0/instructions.md"
        )),
        "internal_skill_documents" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/documents/1.0.0/instructions.md"
        )),
        "internal_skill_pdf" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/pdf/1.0.0/instructions.md"
        )),
        "internal_skill_presentations" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/presentations/1.0.0/instructions.md"
        )),
        "internal_skill_spreadsheets" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/spreadsheets/1.0.0/instructions.md"
        )),
        "internal_skill_template_creator" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/template-creator/1.0.0/instructions.md"
        )),
        "internal_skill_imagegen" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/imagegen/1.0.0/instructions.md"
        )),
        "internal_skill_figma_code_connect" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-code-connect/1.0.0/instructions.md"
        )),
        "internal_skill_figma_create_new_file" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-create-new-file/1.0.0/instructions.md"
        )),
        "internal_skill_figma_design_to_code" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-design-to-code/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_design" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-generate-design/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_diagram" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-generate-diagram/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_library" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-generate-library/1.0.0/instructions.md"
        )),
        "internal_skill_figma_implement_motion" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-implement-motion/1.0.0/instructions.md"
        )),
        "internal_skill_figma_swiftui" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-swiftui/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_figjam" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use-figjam/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_motion" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use-motion/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_slides" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/figma-use-slides/1.0.0/instructions.md"
        )),
        "internal_skill_browser" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/control-in-app-browser/1.0.0/instructions.md"
        )),
        "internal_skill_computer_use" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/computer-use/1.0.0/instructions.md"
        )),
        "internal_skill_excel_live_control" => Some(include_str!(
            "../../../../local_connector_client/skill_bundles/internal/excel-live-control/1.0.0/instructions.md"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_bundle_v2_fingerprint_matches_local_connector() {
        let catalog = internal_skill_catalog().expect("catalog");
        assert_eq!(
            catalog
                .skills
                .iter()
                .filter(|item| item.implementation_status == "ready")
                .count(),
            12
        );
        let rows = catalog
            .skills
            .iter()
            .filter(|item| item.implementation_status == "ready")
            .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            hex::encode(Sha256::digest(rows.as_bytes())),
            "a35f8389f83ffdfaffdf849e7bd505f6444e9de1147cc7741bd7997f7bd9f68d"
        );
    }

    #[test]
    fn all_27_bundled_skill_fingerprints_match_local_connector() {
        let catalog = internal_skill_catalog().expect("catalog");
        assert_eq!(catalog.skills.len(), 27);
        let rows = catalog
            .skills
            .iter()
            .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            hex::encode(Sha256::digest(rows.as_bytes())),
            "223fd6a9576b6c6f90f7c4a7e0f3862d68e8a212c4e308a3701844d0e5398ef9"
        );
    }
}
