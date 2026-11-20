use super::{
    required_identifier, ManualPluginComponent, ManualPluginDirectorySelection,
    ManualPluginSelectionInput, PluginSettingsManager,
};
use crate::plugin_installer::LocalPluginHookSummary;
use chatos_plugin_management_sdk::PluginManifest;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(super) struct ManualPluginSelection {
    pub(super) owner_user_id: String,
    pub(super) presentation_id: String,
    pub(super) generation: u64,
    pub(super) canonical_root: PathBuf,
    pub(super) directory_name: String,
    pub(super) inspected_fingerprint: Option<String>,
    pub(super) operation_id: Option<String>,
    pub(super) discard_after_operation: bool,
}

impl PluginSettingsManager {
    pub(super) fn stage_manual_selection(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        generation: u64,
        canonical_root: PathBuf,
        directory_name: String,
    ) -> Result<ManualPluginDirectorySelection, String> {
        let selection_id = format!("plugin-source-{}", uuid::Uuid::new_v4().simple());
        let mut selections = self
            .manual_selections
            .lock()
            .map_err(|_| "Plugin 手动安装状态不可用。".to_owned())?;
        for selection in selections.values_mut().filter(|selection| {
            selection.owner_user_id == owner_user_id
                && selection.presentation_id == presentation_id
                && selection.operation_id.is_some()
        }) {
            selection.discard_after_operation = true;
        }
        selections.retain(|_, selection| {
            selection.owner_user_id != owner_user_id
                || selection.presentation_id != presentation_id
                || selection.operation_id.is_some()
        });
        while selections.len() >= 64 {
            let removable = selections
                .iter()
                .find_map(|(key, selection)| selection.operation_id.is_none().then(|| key.clone()))
                .ok_or_else(|| "Plugin 手动安装操作过多，请稍后重试。".to_owned())?;
            selections.remove(&removable);
        }
        selections.insert(
            selection_id.clone(),
            ManualPluginSelection {
                owner_user_id: owner_user_id.to_owned(),
                presentation_id: presentation_id.to_owned(),
                generation,
                canonical_root,
                directory_name: directory_name.clone(),
                inspected_fingerprint: None,
                operation_id: None,
                discard_after_operation: false,
            },
        );
        Ok(ManualPluginDirectorySelection {
            selection_id,
            directory_name,
        })
    }

    pub(super) fn begin_manual_operation(
        &self,
        input: &ManualPluginSelectionInput,
        generation: u64,
        expected_fingerprint: Option<&str>,
    ) -> Result<(ManualPluginSelection, String), String> {
        let mut selections = self
            .manual_selections
            .lock()
            .map_err(|_| "Plugin 手动安装状态不可用。".to_owned())?;
        let selection = selections
            .get_mut(&input.selection_id)
            .filter(|selection| {
                selection.owner_user_id == input.owner_user_id
                    && selection.presentation_id == input.presentation_id
                    && selection.generation == generation
            })
            .ok_or_else(manual_selection_expired)?;
        if selection.operation_id.is_some() {
            return Err("Plugin 来源正在处理，请等待当前操作完成。".into());
        }
        if let Some(expected) = expected_fingerprint {
            if selection.inspected_fingerprint.as_deref() != Some(expected) {
                return Err("Plugin 预检结果已失效，请重新预检。".into());
            }
        }
        let operation_id = uuid::Uuid::new_v4().to_string();
        selection.operation_id = Some(operation_id.clone());
        Ok((selection.clone(), operation_id))
    }

    pub(super) fn finish_manual_inspection(
        &self,
        selection_id: &str,
        operation_id: &str,
        fingerprint: Option<String>,
    ) {
        let Ok(mut selections) = self.manual_selections.lock() else {
            return;
        };
        let Some(selection) = selections.get_mut(selection_id) else {
            return;
        };
        if selection.operation_id.as_deref() != Some(operation_id) {
            return;
        }
        if selection.discard_after_operation {
            selections.remove(selection_id);
            return;
        }
        selection.operation_id = None;
        selection.inspected_fingerprint = fingerprint;
    }

    pub(super) fn finish_manual_install(
        &self,
        selection_id: &str,
        operation_id: &str,
        installed: bool,
    ) {
        let Ok(mut selections) = self.manual_selections.lock() else {
            return;
        };
        if selections
            .get(selection_id)
            .and_then(|selection| selection.operation_id.as_deref())
            != Some(operation_id)
        {
            return;
        }
        if installed
            || selections
                .get(selection_id)
                .is_some_and(|selection| selection.discard_after_operation)
        {
            selections.remove(selection_id);
        } else if let Some(selection) = selections.get_mut(selection_id) {
            selection.operation_id = None;
        }
    }

    pub(super) fn discard_manual_selection(&self, owner_user_id: &str, presentation_id: &str) {
        let Ok(mut selections) = self.manual_selections.lock() else {
            return;
        };
        for selection in selections.values_mut().filter(|selection| {
            selection.owner_user_id == owner_user_id
                && selection.presentation_id == presentation_id
                && selection.operation_id.is_some()
        }) {
            selection.discard_after_operation = true;
        }
        selections.retain(|_, selection| {
            selection.owner_user_id != owner_user_id
                || selection.presentation_id != presentation_id
                || selection.operation_id.is_some()
        });
    }
}

pub(super) fn validated_manual_selection_input(
    input: ManualPluginSelectionInput,
) -> Result<ManualPluginSelectionInput, String> {
    Ok(ManualPluginSelectionInput {
        owner_user_id: required_identifier("owner_user_id", input.owner_user_id)?,
        presentation_id: required_identifier("presentation_id", input.presentation_id)?,
        selection_id: required_identifier("selection_id", input.selection_id)?,
    })
}

pub(super) fn prepare_manual_plugin_directory(path: &Path) -> Result<(PathBuf, String), String> {
    if !path.is_absolute() {
        return Err("Plugin 来源必须是绝对目录。".into());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| "Plugin 来源目录不存在。".to_owned())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata_is_reparse(&metadata) {
        return Err("Plugin 来源必须是普通目录，不能是链接或重解析点。".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| "Plugin 来源目录不可用。".to_owned())?;
    let directory_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty() && value.len() <= 240 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "Plugin 来源目录名称无效。".to_owned())?
        .to_owned();
    Ok((canonical, directory_name))
}

pub(super) fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

pub(super) fn manual_plugin_components(
    manifest: &PluginManifest,
    hook_summaries: &[LocalPluginHookSummary],
) -> Vec<ManualPluginComponent> {
    let mut components = manifest
        .mcp_servers
        .iter()
        .map(|server| ManualPluginComponent {
            kind: "mcp",
            display_name: server.component_key().to_owned(),
            hook_count: None,
            events: Vec::new(),
            runtime_status: None,
        })
        .chain(manifest.skills.iter().map(|skill| {
            ManualPluginComponent {
                kind: "skill",
                display_name: Path::new(&skill.path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Skill")
                    .to_owned(),
                hook_count: None,
                events: Vec::new(),
                runtime_status: None,
            }
        }))
        .chain(hook_summaries.iter().map(|hook| ManualPluginComponent {
            kind: "hook",
            display_name: hook.component_key.clone(),
            hook_count: Some(hook.hook_count),
            events: hook.events.clone(),
            runtime_status: Some("ready"),
        }))
        .collect::<Vec<_>>();
    components.sort_by(|left, right| {
        left.kind
            .cmp(right.kind)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    components
}

fn manual_selection_expired() -> String {
    "Plugin 目录选择已失效，请重新选择。".into()
}
