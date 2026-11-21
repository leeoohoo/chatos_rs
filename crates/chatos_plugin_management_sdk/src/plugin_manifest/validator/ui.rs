// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use super::super::components::{
    PluginUiContribution, PluginUiRuntime, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
    PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ, PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES,
    PLUGIN_UI_MAX_ASSETS, PLUGIN_UI_MAX_BRIDGE_CAPABILITIES,
    PLUGIN_UI_RUNTIME_MAX_LAUNCH_TIMEOUT_MS, PLUGIN_UI_RUNTIME_MIN_LAUNCH_TIMEOUT_MS,
    PLUGIN_UI_SURFACE_ARTIFACT_VIEWER, PLUGIN_UI_SURFACE_DETAIL_PANEL,
    PLUGIN_UI_SURFACE_MESSAGE_PANEL, PLUGIN_UI_SURFACE_WORKBENCH,
};
use super::super::validation_support::issue;
use super::super::PluginManifestValidationIssue;
use super::{validate_package_executable, validate_path};

pub(super) fn validate_ui_contribution(
    index: usize,
    ui: &PluginUiContribution,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if !ui.source.path.starts_with("./ui/")
        || !ui.source.path.to_ascii_lowercase().ends_with(".html")
    {
        issue(
            issues,
            format!("ui[{index}].source").as_str(),
            "Plugin UI entrypoint must be an HTML file under ./ui/",
        );
    }
    if ui
        .title
        .as_ref()
        .is_some_and(|title| title.is_empty() || title.len() > 512)
    {
        issue(
            issues,
            format!("ui[{index}].title").as_str(),
            "Plugin UI title must contain at most 128 Unicode characters",
        );
    }
    if let Some(surface) = ui.surface.as_deref() {
        if ![
            PLUGIN_UI_SURFACE_DETAIL_PANEL,
            PLUGIN_UI_SURFACE_MESSAGE_PANEL,
            PLUGIN_UI_SURFACE_WORKBENCH,
            PLUGIN_UI_SURFACE_ARTIFACT_VIEWER,
        ]
        .contains(&surface)
        {
            issue(
                issues,
                format!("ui[{index}].surface").as_str(),
                "Plugin UI surface must be detail_panel, message_panel, workbench, or artifact_viewer",
            );
        }
    }
    if let Some(runtime) = &ui.runtime {
        validate_ui_runtime(index, ui.surface.as_deref(), runtime, issues);
    }
    validate_ui_assets(index, ui, issues);
    validate_bridge_capabilities(index, ui, issues);
    validate_artifact_mime_types(index, ui, issues);
}

fn validate_ui_assets(
    index: usize,
    ui: &PluginUiContribution,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if ui.assets.len() > PLUGIN_UI_MAX_ASSETS {
        issue(
            issues,
            format!("ui[{index}].assets").as_str(),
            format!("Plugin UI assets must contain at most {PLUGIN_UI_MAX_ASSETS} items"),
        );
    }
    let mut assets = HashSet::new();
    for (asset_index, asset) in ui.assets.iter().enumerate() {
        let field = format!("ui[{index}].assets[{asset_index}]");
        validate_path(field.clone(), asset, issues);
        let extension = asset
            .path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        let safe_extension = extension.as_deref().is_some_and(|extension| {
            matches!(
                extension,
                "js" | "mjs"
                    | "css"
                    | "json"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "woff"
                    | "woff2"
            )
        });
        if !asset.path.starts_with("./ui/") || !safe_extension {
            issue(
                issues,
                field.as_str(),
                "Plugin UI asset must use a supported static extension under ./ui/",
            );
        } else if asset.path == ui.source.path || !assets.insert(asset.path.as_str()) {
            issue(
                issues,
                field.as_str(),
                "Plugin UI assets must be unique and cannot repeat the entrypoint",
            );
        }
    }
}

fn validate_bridge_capabilities(
    index: usize,
    ui: &PluginUiContribution,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if ui.bridge_capabilities.len() > PLUGIN_UI_MAX_BRIDGE_CAPABILITIES {
        issue(
            issues,
            format!("ui[{index}].bridgeCapabilities").as_str(),
            format!(
                "Plugin UI bridge capabilities must contain at most {PLUGIN_UI_MAX_BRIDGE_CAPABILITIES} items"
            ),
        );
    }
    let allowed_capabilities = [
        PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
    ];
    let mut capabilities = HashSet::new();
    for (capability_index, capability) in ui.bridge_capabilities.iter().enumerate() {
        if !allowed_capabilities.contains(&capability.as_str()) {
            issue(
                issues,
                format!("ui[{index}].bridgeCapabilities[{capability_index}]").as_str(),
                "Plugin UI bridge capability is not supported",
            );
        } else if !capabilities.insert(capability.as_str()) {
            issue(
                issues,
                format!("ui[{index}].bridgeCapabilities[{capability_index}]").as_str(),
                "duplicate Plugin UI bridge capability",
            );
        }
    }
}

fn validate_artifact_mime_types(
    index: usize,
    ui: &PluginUiContribution,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if ui.artifact_mime_types.len() > PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES {
        issue(
            issues,
            format!("ui[{index}].artifactMimeTypes").as_str(),
            format!(
                "Plugin UI artifact MIME types must contain at most {PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES} items"
            ),
        );
    }
    let mut mime_types = HashSet::new();
    for (mime_index, mime_type) in ui.artifact_mime_types.iter().enumerate() {
        let valid = mime_type.len() <= 128
            && mime_type.split_once('/').is_some_and(|(kind, subtype)| {
                !kind.is_empty()
                    && !subtype.is_empty()
                    && kind.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                    && subtype.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'-' | b'+' | b'.')
                    })
            });
        if !valid {
            issue(
                issues,
                format!("ui[{index}].artifactMimeTypes[{mime_index}]").as_str(),
                "Plugin UI artifact MIME type is invalid",
            );
        } else if !mime_types.insert(mime_type.as_str()) {
            issue(
                issues,
                format!("ui[{index}].artifactMimeTypes[{mime_index}]").as_str(),
                "duplicate Plugin UI artifact MIME type",
            );
        }
    }
}

fn validate_ui_runtime(
    index: usize,
    surface: Option<&str>,
    runtime: &PluginUiRuntime,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    match runtime {
        PluginUiRuntime::LocalHttp {
            bin,
            args,
            health_path,
            launch_timeout_ms,
        } => {
            validate_package_executable(
                format!("ui[{index}].runtime").as_str(),
                "Plugin UI runtime",
                bin,
                args,
                issues,
            );
            if surface != Some(PLUGIN_UI_SURFACE_WORKBENCH) {
                issue(
                    issues,
                    format!("ui[{index}].surface").as_str(),
                    "local_http Plugin UI runtime requires the workbench surface",
                );
            }
            if let Some(path) = health_path {
                let valid = path.starts_with('/')
                    && path.len() <= 2_048
                    && !path.contains(['?', '#', '\0'])
                    && !path.split('/').any(|segment| segment == "..");
                if !valid {
                    issue(
                        issues,
                        format!("ui[{index}].runtime.healthPath").as_str(),
                        "healthPath must be a safe absolute HTTP path",
                    );
                }
            }
            if launch_timeout_ms.is_some_and(|timeout| {
                !(PLUGIN_UI_RUNTIME_MIN_LAUNCH_TIMEOUT_MS..=PLUGIN_UI_RUNTIME_MAX_LAUNCH_TIMEOUT_MS)
                    .contains(&timeout)
            }) {
                issue(
                    issues,
                    format!("ui[{index}].runtime.launchTimeoutMs").as_str(),
                    format!(
                        "launchTimeoutMs must be between {PLUGIN_UI_RUNTIME_MIN_LAUNCH_TIMEOUT_MS} and {PLUGIN_UI_RUNTIME_MAX_LAUNCH_TIMEOUT_MS}"
                    ),
                );
            }
        }
    }
}
