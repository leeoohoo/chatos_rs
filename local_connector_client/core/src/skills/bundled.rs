// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sha2::{Digest, Sha256};

use super::InternalSkillCatalogItem;

pub(super) fn internal_skill_bundle_hash(item: &InternalSkillCatalogItem) -> String {
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

pub(super) fn internal_skill_manifest(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "internal_skill_plugin_creator" => Some(include_str!(
            "../../../skill_bundles/internal/plugin-creator/1.0.0/skill.json"
        )),
        "internal_skill_openai_docs" => Some(include_str!(
            "../../../skill_bundles/internal/openai-docs/1.0.0/skill.json"
        )),
        "internal_skill_skill_creator" => Some(include_str!(
            "../../../skill_bundles/internal/skill-creator/1.0.0/skill.json"
        )),
        "internal_skill_skill_installer" => Some(include_str!(
            "../../../skill_bundles/internal/skill-installer/1.0.0/skill.json"
        )),
        "internal_skill_remotion" => Some(include_str!(
            "../../../skill_bundles/internal/remotion-best-practices/1.0.0/skill.json"
        )),
        "internal_skill_visualize" => Some(include_str!(
            "../../../skill_bundles/internal/visualize/1.0.0/skill.json"
        )),
        "internal_skill_documents" => Some(include_str!(
            "../../../skill_bundles/internal/documents/1.0.0/skill.json"
        )),
        "internal_skill_pdf" => Some(include_str!(
            "../../../skill_bundles/internal/pdf/1.0.0/skill.json"
        )),
        "internal_skill_presentations" => Some(include_str!(
            "../../../skill_bundles/internal/presentations/1.0.0/skill.json"
        )),
        "internal_skill_spreadsheets" => Some(include_str!(
            "../../../skill_bundles/internal/spreadsheets/1.0.0/skill.json"
        )),
        "internal_skill_template_creator" => Some(include_str!(
            "../../../skill_bundles/internal/template-creator/1.0.0/skill.json"
        )),
        "internal_skill_imagegen" => Some(include_str!(
            "../../../skill_bundles/internal/imagegen/1.0.0/skill.json"
        )),
        "internal_skill_figma_code_connect" => Some(include_str!(
            "../../../skill_bundles/internal/figma-code-connect/1.0.0/skill.json"
        )),
        "internal_skill_figma_create_new_file" => Some(include_str!(
            "../../../skill_bundles/internal/figma-create-new-file/1.0.0/skill.json"
        )),
        "internal_skill_figma_design_to_code" => Some(include_str!(
            "../../../skill_bundles/internal/figma-design-to-code/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_design" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-design/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_diagram" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-diagram/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_library" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-library/1.0.0/skill.json"
        )),
        "internal_skill_figma_implement_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-implement-motion/1.0.0/skill.json"
        )),
        "internal_skill_figma_swiftui" => Some(include_str!(
            "../../../skill_bundles/internal/figma-swiftui/1.0.0/skill.json"
        )),
        "internal_skill_figma_use" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_figjam" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-figjam/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-motion/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_slides" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-slides/1.0.0/skill.json"
        )),
        "internal_skill_browser" => Some(include_str!(
            "../../../skill_bundles/internal/control-in-app-browser/1.0.0/skill.json"
        )),
        "internal_skill_computer_use" => Some(include_str!(
            "../../../skill_bundles/internal/computer-use/1.0.0/skill.json"
        )),
        "internal_skill_excel_live_control" => Some(include_str!(
            "../../../skill_bundles/internal/excel-live-control/1.0.0/skill.json"
        )),
        _ => None,
    }
}

pub(super) fn internal_skill_instructions(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "internal_skill_plugin_creator" => Some(include_str!(
            "../../../skill_bundles/internal/plugin-creator/1.0.0/instructions.md"
        )),
        "internal_skill_openai_docs" => Some(include_str!(
            "../../../skill_bundles/internal/openai-docs/1.0.0/instructions.md"
        )),
        "internal_skill_skill_creator" => Some(include_str!(
            "../../../skill_bundles/internal/skill-creator/1.0.0/instructions.md"
        )),
        "internal_skill_skill_installer" => Some(include_str!(
            "../../../skill_bundles/internal/skill-installer/1.0.0/instructions.md"
        )),
        "internal_skill_remotion" => Some(include_str!(
            "../../../skill_bundles/internal/remotion-best-practices/1.0.0/instructions.md"
        )),
        "internal_skill_visualize" => Some(include_str!(
            "../../../skill_bundles/internal/visualize/1.0.0/instructions.md"
        )),
        "internal_skill_documents" => Some(include_str!(
            "../../../skill_bundles/internal/documents/1.0.0/instructions.md"
        )),
        "internal_skill_pdf" => Some(include_str!(
            "../../../skill_bundles/internal/pdf/1.0.0/instructions.md"
        )),
        "internal_skill_presentations" => Some(include_str!(
            "../../../skill_bundles/internal/presentations/1.0.0/instructions.md"
        )),
        "internal_skill_spreadsheets" => Some(include_str!(
            "../../../skill_bundles/internal/spreadsheets/1.0.0/instructions.md"
        )),
        "internal_skill_template_creator" => Some(include_str!(
            "../../../skill_bundles/internal/template-creator/1.0.0/instructions.md"
        )),
        "internal_skill_imagegen" => Some(include_str!(
            "../../../skill_bundles/internal/imagegen/1.0.0/instructions.md"
        )),
        "internal_skill_figma_code_connect" => Some(include_str!(
            "../../../skill_bundles/internal/figma-code-connect/1.0.0/instructions.md"
        )),
        "internal_skill_figma_create_new_file" => Some(include_str!(
            "../../../skill_bundles/internal/figma-create-new-file/1.0.0/instructions.md"
        )),
        "internal_skill_figma_design_to_code" => Some(include_str!(
            "../../../skill_bundles/internal/figma-design-to-code/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_design" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-design/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_diagram" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-diagram/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_library" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-library/1.0.0/instructions.md"
        )),
        "internal_skill_figma_implement_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-implement-motion/1.0.0/instructions.md"
        )),
        "internal_skill_figma_swiftui" => Some(include_str!(
            "../../../skill_bundles/internal/figma-swiftui/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_figjam" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-figjam/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-motion/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_slides" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-slides/1.0.0/instructions.md"
        )),
        "internal_skill_browser" => Some(include_str!(
            "../../../skill_bundles/internal/control-in-app-browser/1.0.0/instructions.md"
        )),
        "internal_skill_computer_use" => Some(include_str!(
            "../../../skill_bundles/internal/computer-use/1.0.0/instructions.md"
        )),
        "internal_skill_excel_live_control" => Some(include_str!(
            "../../../skill_bundles/internal/excel-live-control/1.0.0/instructions.md"
        )),
        _ => None,
    }
}
