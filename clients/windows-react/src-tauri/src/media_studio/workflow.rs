use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chatos_ai_runtime::simple_prompt::SimplePromptOptions;
use chatos_ai_runtime::{
    build_responses_text_input, run_compatible_prompt_with, AiRequestHandler, ModelRuntimeConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::{
    app_data_transaction::{ensure_owned_directory, replace_owned_file},
    authentication::{AuthenticationManager, GatewayModelConfig},
    host_lifecycle::validate_owner,
};

const MAXIMUM_MANIFEST_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_TEXT_LENGTH: usize = 20_000;
const VIDEO_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
const MEDIA_WORKFLOW_SYSTEM_PROMPT: &str = r#"你是 AI 创作工作流中的文本节点。完整执行节点要求，输出可直接交给下游文本、图片或视频节点使用的成品，而不是简短建议、过程说明或任务摘要。

准确分析所有提供的图片和当前工作文本，不臆造不可见信息。严格遵循用户指定的语言、格式和篇幅；用户未限制篇幅时，应提供完成任务所需的充分细节。

当节点要求生成图片或视频提示词时，直接给出完整、可复制使用的专业提示词：明确每份参考素材的用途、需要保留与修改的内容，并覆盖主体、构图、姿态、镜头、光线、色彩、材质、环境及必要的约束或负面要求。不要只解释应该怎么写。"#;

fn validate_workflow_text_capability(
    model: &GatewayModelConfig,
) -> Result<(String, usize, usize, String), String> {
    let context_window_tokens = model
        .context_window_tokens
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "文本模型缺少有效上下文窗口。".to_owned())?;
    let output_reserve_tokens = model
        .output_reserve_tokens
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "文本模型缺少有效输出预留。".to_owned())?;
    let model_capability_revision = model
        .model_capability_revision
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "文本模型缺少 capability revision。".to_owned())?;
    let protocol_overhead_estimator_id = model
        .protocol_overhead_estimator_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "文本模型缺少协议开销估算器。".to_owned())?;
    if output_reserve_tokens >= context_window_tokens
        || model.max_output_tokens.is_some_and(|value| {
            value <= 0
                || u128::try_from(value).map_or(true, |value| value > output_reserve_tokens as u128)
        })
    {
        return Err("文本模型能力快照无效。".into());
    }
    Ok((
        model_capability_revision,
        context_window_tokens,
        output_reserve_tokens,
        protocol_overhead_estimator_id,
    ))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MediaWorkflowNodeKind {
    Text,
    Image,
    Video,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaWorkflowImageReference {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub filename: String,
    pub byte_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaWorkflowNode {
    pub id: String,
    pub kind: MediaWorkflowNodeKind,
    pub model_config_id: String,
    pub prompt: String,
    #[serde(default)]
    pub reference_images: Vec<MediaWorkflowImageReference>,
    #[serde(default = "default_image_size")]
    pub image_size: String,
    #[serde(default = "default_video_size")]
    pub video_size: String,
    #[serde(default = "default_video_ratio")]
    pub video_ratio: String,
    #[serde(default = "default_video_seconds")]
    pub video_seconds: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaWorkflow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub input_prompt: String,
    #[serde(default)]
    pub input_images: Vec<MediaWorkflowImageReference>,
    pub nodes: Vec<MediaWorkflowNode>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

fn default_image_size() -> String {
    "1024x1024".into()
}

fn default_video_size() -> String {
    "768P".into()
}

fn default_video_ratio() -> String {
    "16:9".into()
}

fn default_video_seconds() -> u16 {
    4
}

pub(crate) fn validate_workflow(workflow: &MediaWorkflow) -> Result<(), String> {
    if Uuid::parse_str(&workflow.id).is_err() {
        return Err("工作流标识无效。".into());
    }
    let name = workflow.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err("工作流名称无效。".into());
    }
    if workflow.input_prompt.chars().count() > MAXIMUM_TEXT_LENGTH {
        return Err("工作流输入提示词过长。".into());
    }
    if workflow.nodes.is_empty() || workflow.nodes.len() > 8 {
        return Err("工作流必须包含 1–8 个模型节点。".into());
    }
    let mut node_ids = HashSet::new();
    let mut video_index = None;
    for (index, node) in workflow.nodes.iter().enumerate() {
        if Uuid::parse_str(&node.id).is_err() || !node_ids.insert(node.id.as_str()) {
            return Err("工作流包含重复或无效节点。".into());
        }
        if node.model_config_id.trim().is_empty() || node.model_config_id.len() > 1_024 {
            return Err("节点缺少有效模型。".into());
        }
        let prompt = node.prompt.trim();
        if prompt.is_empty() || prompt.chars().count() > MAXIMUM_TEXT_LENGTH {
            return Err("节点提示词不能为空或过长。".into());
        }
        let maximum_references = if node.kind == MediaWorkflowNodeKind::Video {
            if video_index.replace(index).is_some() || index + 1 != workflow.nodes.len() {
                return Err("视频节点只能出现一次并且必须位于流程末尾。".into());
            }
            if !matches!(node.video_size.as_str(), "768P" | "2K")
                || !matches!(
                    node.video_ratio.as_str(),
                    "16:9" | "9:16" | "1:1" | "4:3" | "3:4" | "21:9"
                )
                || !(4..=15).contains(&node.video_seconds)
            {
                return Err("视频节点尺寸、比例或时长无效。".into());
            }
            1
        } else {
            8
        };
        validate_references(&node.reference_images, maximum_references)?;
    }
    validate_references(&workflow.input_images, 8)
}

fn validate_references(
    references: &[MediaWorkflowImageReference],
    maximum: usize,
) -> Result<(), String> {
    if references.len() > maximum {
        return Err("工作流参考图数量超过限制。".into());
    }
    let mut ids = HashSet::new();
    for reference in references {
        if Uuid::parse_str(&reference.id).is_err()
            || !ids.insert(reference.id.as_str())
            || reference.name.is_empty()
            || reference.name.chars().count() > 255
            || !reference
                .mime_type
                .to_ascii_lowercase()
                .starts_with("image/")
            || reference.filename.is_empty()
            || reference.filename
                != Path::new(&reference.filename)
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or_default()
            || reference.filename.starts_with('.')
            || reference.byte_count == 0
            || reference.byte_count > MAXIMUM_IMAGE_BYTES
        {
            return Err("工作流参考图无效。".into());
        }
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct MediaStudioWorkflowStore {
    root: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl MediaStudioWorkflowStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let root = app_data_directory.join("MediaStudio").join("Workflows");
        ensure_owned_directory(&root).map_err(|_| "无法创建工作流目录。".to_owned())?;
        Ok(Self {
            root,
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    pub(crate) fn list(&self, owner: &str) -> Result<Vec<MediaWorkflow>, String> {
        validate_owner(owner)?;
        let directory = self.owner_directory(owner);
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut workflows = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|_| "无法读取工作流目录。")? {
            let path = entry.map_err(|_| "无法读取工作流目录。")?.path();
            if !is_plain_directory(&path) {
                continue;
            }
            let Ok(workflow) = self.load(&path, owner) else {
                continue;
            };
            workflows.push(workflow);
        }
        workflows.sort_by(|left, right| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms));
        Ok(workflows)
    }

    pub(crate) fn save(&self, owner: &str, workflow: &MediaWorkflow) -> Result<bool, String> {
        validate_owner(owner)?;
        validate_workflow(workflow)?;
        let _write_guard = self
            .write_lock
            .lock()
            .map_err(|_| "工作流保存状态不可用。".to_owned())?;
        let folder = self.workflow_directory(owner, &workflow.id)?;
        let manifest = folder.join("workflow.json");
        let previous_manifest_modified_at = fs::symlink_metadata(&manifest)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        if manifest.exists() {
            let saved = self.load(&folder, owner)?;
            if saved.updated_at_unix_ms > workflow.updated_at_unix_ms {
                return Ok(false);
            }
        }
        let assets = folder.join("assets");
        ensure_owned_directory(&assets).map_err(|_| "无法创建工作流资产目录。".to_owned())?;
        let bytes = serde_json::to_vec(workflow).map_err(|_| "无法编码工作流。".to_owned())?;
        if bytes.len() > MAXIMUM_MANIFEST_BYTES {
            return Err("工作流清单过大。".into());
        }
        replace_owned_file(&folder, &manifest, ".workflow-", &bytes)
            .map_err(|_| "无法保存工作流。".to_owned())?;
        self.cleanup_assets(&assets, workflow, previous_manifest_modified_at)?;
        Ok(true)
    }

    pub(crate) fn delete(&self, owner: &str, id: &str) -> Result<(), String> {
        validate_owner(owner)?;
        let _write_guard = self
            .write_lock
            .lock()
            .map_err(|_| "工作流保存状态不可用。".to_owned())?;
        let id = Uuid::parse_str(id).map_err(|_| "工作流标识无效。".to_owned())?;
        let folder = self.owner_directory(owner).join(id.to_string());
        if !folder.exists() {
            return Ok(());
        }
        if !is_plain_directory(&folder) {
            return Err("工作流目录无效。".into());
        }
        fs::remove_dir_all(folder).map_err(|_| "无法删除工作流。".to_owned())
    }

    pub(crate) fn resolve_images(
        &self,
        owner: &str,
        references: &[MediaWorkflowImageReference],
        workflow_id: &str,
    ) -> Result<Vec<super::provider::ImageGenerationInput>, String> {
        validate_owner(owner)?;
        let folder = self.workflow_directory_for_read(owner, workflow_id)?;
        let assets = folder.join("assets");
        references
            .iter()
            .map(|reference| {
                let path = assets.join(&reference.filename);
                let metadata =
                    fs::symlink_metadata(&path).map_err(|_| "工作流参考图不存在。".to_owned())?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() != reference.byte_count as u64
                {
                    return Err("工作流参考图无效。".into());
                }
                let bytes = fs::read(&path).map_err(|_| "工作流参考图读取失败。".to_owned())?;
                if bytes.len() != reference.byte_count {
                    return Err("工作流参考图大小已变化。".into());
                }
                Ok(super::provider::ImageGenerationInput {
                    name: reference.name.clone(),
                    mime_type: reference.mime_type.clone(),
                    bytes,
                })
            })
            .collect()
    }

    fn load(&self, folder: &Path, owner: &str) -> Result<MediaWorkflow, String> {
        let manifest = folder.join("workflow.json");
        let metadata =
            fs::symlink_metadata(&manifest).map_err(|_| "工作流清单无效。".to_owned())?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() as usize > MAXIMUM_MANIFEST_BYTES
        {
            return Err("工作流清单无效。".into());
        }
        let bytes = fs::read(&manifest).map_err(|_| "工作流清单无效。".to_owned())?;
        let mut workflow: MediaWorkflow =
            serde_json::from_slice(&bytes).map_err(|_| "工作流清单无效。".to_owned())?;
        normalize_legacy_workflow_defaults(&mut workflow);
        if workflow.id
            != folder
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
        {
            return Err("工作流清单标识不匹配。".into());
        }
        validate_workflow(&workflow)?;
        self.resolve_images(
            owner,
            &workflow
                .input_images
                .iter()
                .chain(
                    workflow
                        .nodes
                        .iter()
                        .flat_map(|node| node.reference_images.iter()),
                )
                .cloned()
                .collect::<Vec<_>>(),
            &workflow.id,
        )?;
        Ok(workflow)
    }

    fn cleanup_assets(
        &self,
        assets: &Path,
        workflow: &MediaWorkflow,
        previous_manifest_modified_at: Option<SystemTime>,
    ) -> Result<(), String> {
        let referenced: HashSet<&str> = workflow
            .input_images
            .iter()
            .chain(
                workflow
                    .nodes
                    .iter()
                    .flat_map(|node| node.reference_images.iter()),
            )
            .map(|reference| reference.filename.as_str())
            .collect();
        for entry in fs::read_dir(assets).map_err(|_| "无法读取工作流资产目录。".to_owned())?
        {
            let path = entry.map_err(|_| "无法读取工作流资产目录。")?.path();
            let name = path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            if referenced.contains(name) {
                continue;
            }
            if !is_plain_file(&path) {
                return Err("工作流资产无效。".into());
            }
            let Some(previous_manifest_modified_at) = previous_manifest_modified_at else {
                continue;
            };
            let modified_at = fs::symlink_metadata(&path)
                .and_then(|metadata| metadata.modified())
                .map_err(|_| "无法读取工作流资产时间。".to_owned())?;
            if modified_at > previous_manifest_modified_at {
                continue;
            }
            fs::remove_file(path).map_err(|_| "无法清理工作流资产。".to_owned())?;
        }
        Ok(())
    }

    fn owner_directory(&self, owner: &str) -> PathBuf {
        self.root
            .join("Accounts")
            .join(super::history::owner_hash(owner))
    }

    fn workflow_directory(&self, owner: &str, id: &str) -> Result<PathBuf, String> {
        let id = Uuid::parse_str(id).map_err(|_| "工作流标识无效。".to_owned())?;
        let directory = self.owner_directory(owner).join(id.to_string());
        ensure_owned_directory(&directory).map_err(|_| "无法创建工作流目录。".to_owned())?;
        Ok(directory)
    }

    fn workflow_directory_for_read(&self, owner: &str, id: &str) -> Result<PathBuf, String> {
        let id = Uuid::parse_str(id).map_err(|_| "工作流标识无效。".to_owned())?;
        let directory = self.owner_directory(owner).join(id.to_string());
        if !is_plain_directory(&directory) {
            return Err("工作流不存在。".into());
        }
        Ok(directory)
    }
}

fn normalize_legacy_workflow_defaults(workflow: &mut MediaWorkflow) {
    for node in &mut workflow.nodes {
        if node.video_size == "1280x720" {
            node.video_size = default_video_size();
        }
    }
}

fn is_plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn is_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowDeleteInput {
    owner_user_id: String,
    workflow_id: String,
}

#[tauri::command]
pub(crate) fn list_media_studio_workflows(
    owner_user_id: String,
    manager: State<'_, super::MediaStudioManager>,
) -> Result<Vec<MediaWorkflow>, String> {
    manager.list_workflows(&owner_user_id)
}

#[tauri::command]
pub(crate) fn save_media_studio_workflow(
    owner_user_id: String,
    workflow: MediaWorkflow,
    manager: State<'_, super::MediaStudioManager>,
) -> Result<bool, String> {
    manager.save_workflow(&owner_user_id, &workflow)
}

#[tauri::command]
pub(crate) fn delete_media_studio_workflow(
    input: WorkflowDeleteInput,
    manager: State<'_, super::MediaStudioManager>,
) -> Result<(), String> {
    manager.delete_workflow(&input.owner_user_id, &input.workflow_id)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MediaWorkflowRunRequest {
    pub owner_user_id: String,
    pub workflow_id: String,
    pub client_request_id: String,
}

const MEDIA_WORKFLOW_RUN_EVENT: &str = "media-workflow-run";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum MediaWorkflowNodeRunResult {
    Text {
        text: String,
    },
    Images {
        item: super::history::ImageHistoryItem,
    },
    Video {
        item: super::video_history::VideoHistoryItem,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaWorkflowCompletedNode {
    node_id: String,
    result: MediaWorkflowNodeRunResult,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaWorkflowNodeRunEvent {
    owner_user_id: String,
    workflow_id: String,
    client_request_id: String,
    node_id: String,
    phase: String,
    progress: Option<String>,
    result: Option<MediaWorkflowNodeRunResult>,
    error_message: Option<String>,
}

struct WorkflowRunEventScope<'a> {
    app: &'a AppHandle,
    request: &'a MediaWorkflowRunRequest,
    workflow_id: &'a str,
    node_id: &'a str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaWorkflowRunResult {
    pub workflow_id: String,
    pub client_request_id: String,
    pub status: String,
    pub final_text: String,
    pub image_record_ids: Vec<String>,
    pub video_record_ids: Vec<String>,
    pub node_results: Vec<MediaWorkflowCompletedNode>,
    pub error_message: Option<String>,
}

fn emit_workflow_node_event(
    scope: &WorkflowRunEventScope<'_>,
    phase: &str,
    progress: Option<String>,
    result: Option<MediaWorkflowNodeRunResult>,
    error_message: Option<String>,
) -> Result<(), String> {
    scope
        .app
        .emit(
            MEDIA_WORKFLOW_RUN_EVENT,
            MediaWorkflowNodeRunEvent {
                owner_user_id: scope.request.owner_user_id.clone(),
                workflow_id: scope.workflow_id.to_owned(),
                client_request_id: scope.request.client_request_id.clone(),
                node_id: scope.node_id.to_owned(),
                phase: phase.to_owned(),
                progress,
                result,
                error_message,
            },
        )
        .map_err(|_| "工作流节点状态发布失败。".to_owned())
}

#[tauri::command]
pub(crate) async fn run_media_studio_workflow(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, super::MediaStudioManager>,
    request: MediaWorkflowRunRequest,
) -> Result<MediaWorkflowRunResult, String> {
    validate_owner(&request.owner_user_id)?;
    if request.client_request_id.trim().is_empty() || request.client_request_id.len() > 1_024 {
        return Err("工作流运行请求标识无效。".into());
    }
    let workflow = manager
        .list_workflows(&request.owner_user_id)?
        .into_iter()
        .find(|workflow| workflow.id == request.workflow_id)
        .ok_or_else(|| "工作流不存在。".to_owned())?;
    validate_workflow(&workflow)?;
    let generation = authentication
        .session_generation_for_owner(&request.owner_user_id)
        .await?;
    let models = resolve_workflow_models(
        &authentication,
        &request.owner_user_id,
        generation,
        &workflow,
    )
    .await?;
    let preview_token = manager.activate_session(&request.owner_user_id, generation)?;
    // Workflow execution starts at the first model node. The legacy input fields
    // remain deserializable for compatibility, but are intentionally not part of
    // the execution snapshot (matching the macOS runner).
    let mut current_text = String::new();
    let mut current_images = Vec::new();
    let mut image_record_ids = Vec::new();
    let mut video_record_ids = Vec::new();
    let mut node_results = Vec::new();
    for node in &workflow.nodes {
        let event_scope = WorkflowRunEventScope {
            app: &app,
            request: &request,
            workflow_id: &workflow.id,
            node_id: &node.id,
        };
        emit_workflow_node_event(&event_scope, "running", None, None, None)?;
        let node_result: Result<MediaWorkflowNodeRunResult, String> = async {
            authentication
                .require_session_generation(&request.owner_user_id, generation)
                .await?;
            let references = manager.workflows.resolve_images(
                &request.owner_user_id,
                &node.reference_images,
                &workflow.id,
            )?;
            let prompt = format!(
                "节点要求：\n{}\n\n当前工作文本：\n{}",
                node.prompt.trim(),
                current_text
            );
            let model = models
                .get(&node.model_config_id)
                .cloned()
                .ok_or_else(|| "工作流模型快照不完整。".to_owned())?;
            match node.kind {
                MediaWorkflowNodeKind::Text => {
                    current_text = generate_workflow_text(
                        &authentication,
                        &manager,
                        &request.owner_user_id,
                        generation,
                        model,
                        &prompt,
                        &current_images,
                        &references,
                    )
                    .await?;
                    Ok(MediaWorkflowNodeRunResult::Text {
                        text: current_text.clone(),
                    })
                }
                MediaWorkflowNodeKind::Image => {
                    let input = super::provider::ImageGenerationRequest {
                        model_config_id: node.model_config_id.clone(),
                        prompt: prompt.clone(),
                        size: Some(node.image_size.clone()),
                        count: 1,
                        reference_images: current_images
                            .iter()
                            .cloned()
                            .chain(references.iter().cloned())
                            .collect(),
                        client_request_id: format!("{}-{}", request.client_request_id, node.id),
                    };
                    if input.reference_images.len() > 8 {
                        return Err("当前图片与节点参考图合计不能超过 8 张。".into());
                    }
                    let (item, next_image) = generate_workflow_image(
                        &authentication,
                        &manager,
                        &request.owner_user_id,
                        generation,
                        input,
                        model,
                        &preview_token,
                    )
                    .await?;
                    image_record_ids.push(item.id.clone());
                    current_images = vec![next_image];
                    Ok(MediaWorkflowNodeRunResult::Images { item })
                }
                MediaWorkflowNodeKind::Video => {
                    if current_images.len() + references.len() > 1 {
                        return Err("视频节点最多只能接收一张首帧图片。".into());
                    }
                    let input_image =
                        current_images
                            .first()
                            .or_else(|| references.first())
                            .map(|image| super::video_provider::VideoInput {
                                mime_type: image.mime_type.clone(),
                                bytes: image.bytes.clone(),
                            });
                    let item = generate_workflow_video(
                        &event_scope,
                        &authentication,
                        &manager,
                        &request.owner_user_id,
                        generation,
                        super::video_provider::VideoGenerationRequest {
                            model_config_id: node.model_config_id.clone(),
                            prompt,
                            size: node.video_size.clone(),
                            seconds: node.video_seconds as u8,
                            ratio: node.video_ratio.clone(),
                            input_image,
                            last_frame_image: None,
                            reference_video: None,
                            reference_audio: None,
                            reference_purpose:
                                super::video_provider::VideoReferencePurpose::Reference,
                            client_request_id: format!("{}-{}", request.client_request_id, node.id),
                        },
                        model,
                        &preview_token,
                    )
                    .await?;
                    video_record_ids.push(item.id.clone());
                    Ok(MediaWorkflowNodeRunResult::Video { item })
                }
            }
        }
        .await;
        match node_result {
            Ok(result) => {
                emit_workflow_node_event(
                    &event_scope,
                    "completed",
                    None,
                    Some(result.clone()),
                    None,
                )?;
                node_results.push(MediaWorkflowCompletedNode {
                    node_id: node.id.clone(),
                    result,
                });
            }
            Err(error) => {
                let _ = emit_workflow_node_event(
                    &event_scope,
                    "failed",
                    None,
                    None,
                    Some(error.clone()),
                );
                return Err(error);
            }
        }
    }
    Ok(MediaWorkflowRunResult {
        workflow_id: workflow.id,
        client_request_id: request.client_request_id,
        status: "completed".into(),
        final_text: current_text,
        image_record_ids,
        video_record_ids,
        node_results,
        error_message: None,
    })
}

async fn resolve_workflow_models(
    authentication: &AuthenticationManager,
    owner: &str,
    generation: u64,
    workflow: &MediaWorkflow,
) -> Result<HashMap<String, GatewayModelConfig>, String> {
    let mut ids = workflow
        .nodes
        .iter()
        .map(|node| node.model_config_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let resolved = authentication
        .resolved_models_for_owner(owner, ids.clone())
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(owner, generation)
        .await?;
    let models = resolved
        .into_iter()
        .map(|model| (model.id.clone(), model))
        .collect::<HashMap<_, _>>();
    if models.len() != ids.len() || ids.iter().any(|id| !models.contains_key(id)) {
        return Err("工作流中的模型已不可用。".into());
    }
    validate_workflow_models(owner, workflow, &models)?;
    Ok(models)
}

fn validate_workflow_models(
    owner: &str,
    workflow: &MediaWorkflow,
    models: &HashMap<String, GatewayModelConfig>,
) -> Result<(), String> {
    use chatos_service_runtime::model_workload::ModelWorkload;

    let mut has_images = false;
    for node in &workflow.nodes {
        let model = models
            .get(&node.model_config_id)
            .ok_or_else(|| "工作流中的模型已不可用。".to_owned())?;
        if model.owner_user_id != owner
            || model.id != node.model_config_id
            || !model.enabled
            || !model.has_api_key
        {
            return Err("工作流模型配置与当前账号不匹配。".into());
        }
        match node.kind {
            MediaWorkflowNodeKind::Text => {
                if ModelWorkload::for_model(&model.model) != ModelWorkload::Chat {
                    return Err("文本节点所选模型不支持文本生成。".into());
                }
                validate_workflow_text_capability(model)?;
                if (has_images || !node.reference_images.is_empty()) && !model.supports_images {
                    return Err("文本节点所选模型不支持读取当前图片输入。".into());
                }
            }
            MediaWorkflowNodeKind::Image => {
                super::provider::resolve_image_model(owner, &node.model_config_id, model.clone())?;
                has_images = true;
            }
            MediaWorkflowNodeKind::Video => {
                super::video_provider::resolve_video_model(
                    owner,
                    &node.model_config_id,
                    model.clone(),
                )?;
            }
        }
    }
    Ok(())
}

async fn generate_workflow_video(
    event_scope: &WorkflowRunEventScope<'_>,
    authentication: &AuthenticationManager,
    manager: &super::MediaStudioManager,
    owner: &str,
    generation: u64,
    request: super::video_provider::VideoGenerationRequest,
    model: GatewayModelConfig,
    preview_token: &str,
) -> Result<super::video_history::VideoHistoryItem, String> {
    let runtime =
        super::video_provider::resolve_video_model(owner, &request.model_config_id, model)?;
    let wire = super::video_provider::make_video_create_request(&runtime, &request)?;
    let response = manager
        .provider_client
        .post(wire.url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .bearer_auth(&runtime.api_key)
        .body(wire.body)
        .send()
        .await
        .map_err(|_| "无法连接视频模型服务。".to_owned())?;
    let status = response.status();
    let body = super::read_bounded_response(
        response,
        super::video_provider::MAXIMUM_VIDEO_JOB_RESPONSE_BYTES,
        "无法读取视频模型响应。",
        "模型返回的视频任务数据过大。",
    )
    .await?;
    if !status.is_success() {
        return Err(format!("创建视频任务失败（HTTP {}）。", status.as_u16()));
    }
    let provider_job = super::video_provider::parse_video_job(&body, &runtime.api_key)?;
    authentication
        .require_session_generation(owner, generation)
        .await?;
    let snapshot = manager.video_jobs.register(
        owner,
        generation,
        event_scope.workflow_id,
        request,
        runtime,
        provider_job,
    )?;
    emit_workflow_video_progress(event_scope, &snapshot)?;
    if snapshot.terminal {
        return workflow_video_snapshot_result(snapshot);
    }
    let mut snapshot = snapshot;
    loop {
        authentication
            .require_session_generation(owner, generation)
            .await?;
        let start = manager.video_jobs.begin_poll(
            owner,
            generation,
            event_scope.workflow_id,
            &snapshot.job_id,
        )?;
        let super::video_job::VideoPollStart::Work(work) = start else {
            let super::video_job::VideoPollStart::Snapshot(snapshot) = start else {
                unreachable!()
            };
            let snapshot = *snapshot;
            if snapshot.terminal {
                return workflow_video_snapshot_result(snapshot);
            }
            continue;
        };
        snapshot = match super::advance_video_job(
            authentication,
            manager,
            owner,
            generation,
            preview_token,
            &work,
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = manager.video_jobs.fail(&work, error.clone());
                return Err(error);
            }
        };
        emit_workflow_video_progress(event_scope, &snapshot)?;
        if snapshot.terminal {
            return workflow_video_snapshot_result(snapshot);
        }
        // Keep workflow video polling aligned with the standalone Windows flow and macOS.
        // MiniMax jobs commonly take several minutes; polling faster while retaining the
        // shared 180-poll ceiling would turn that ceiling into a false early timeout.
        tokio::time::sleep(VIDEO_POLL_INTERVAL).await;
    }
}

fn emit_workflow_video_progress(
    event_scope: &WorkflowRunEventScope<'_>,
    snapshot: &super::video_job::MediaStudioVideoJobSnapshot,
) -> Result<(), String> {
    let progress = snapshot
        .percent
        .map(|percent| format!("{percent:.0}%"))
        .or_else(|| Some(snapshot.status.clone()));
    emit_workflow_node_event(event_scope, "running", progress, None, None)
}

fn workflow_video_snapshot_result(
    snapshot: super::video_job::MediaStudioVideoJobSnapshot,
) -> Result<super::video_history::VideoHistoryItem, String> {
    if let Some(result) = snapshot.result {
        return Ok(result);
    }
    Err(snapshot
        .error_message
        .unwrap_or_else(|| "视频生成失败。".into()))
}

async fn generate_workflow_text(
    authentication: &AuthenticationManager,
    manager: &super::MediaStudioManager,
    owner: &str,
    generation: u64,
    model: GatewayModelConfig,
    prompt: &str,
    current_images: &[super::provider::ImageGenerationInput],
    references: &[super::provider::ImageGenerationInput],
) -> Result<String, String> {
    if chatos_service_runtime::model_workload::ModelWorkload::for_model(&model.model)
        != chatos_service_runtime::model_workload::ModelWorkload::Chat
    {
        return Err("所选模型不支持文本生成。".into());
    }
    if !model.enabled
        || !model.has_api_key
        || !(model.supports_images || current_images.is_empty() && references.is_empty())
    {
        return Err("文本模型不可用或不支持当前图片输入。".into());
    }
    let api_key = model
        .api_key
        .clone()
        .ok_or_else(|| "模型配置没有可用凭据。".to_owned())?;
    let base_url = model
        .base_url
        .clone()
        .ok_or_else(|| "模型配置没有 API 地址。".to_owned())?;
    let (
        model_capability_revision,
        context_window_tokens,
        output_reserve_tokens,
        protocol_overhead_estimator_id,
    ) = validate_workflow_text_capability(&model)?;
    let config = ModelRuntimeConfig {
        model_capability_revision,
        context_window_tokens,
        output_reserve_tokens,
        protocol_overhead_estimator_id,
        tokenizer_id: model.tokenizer_id.clone(),
        base_url,
        api_key,
        model: model.model.clone(),
        provider: model.provider.clone(),
        supports_responses: model.supports_responses,
        supports_images: Some(model.supports_images),
        max_output_tokens: model.max_output_tokens,
        thinking_level: model.task_thinking_level.clone(),
        ..Default::default()
    };
    let mut images = current_images.to_vec();
    images.extend_from_slice(references);
    let handler = AiRequestHandler::from_client(manager.provider_client.clone());
    let response = run_compatible_prompt_with(
        &handler,
        &config,
        prompt,
        SimplePromptOptions {
            system_prompt: Some(MEDIA_WORKFLOW_SYSTEM_PROMPT.to_owned()),
            max_output_tokens: model.max_output_tokens,
            ..Default::default()
        },
        |value, input_as_list| {
            if images.is_empty() {
                return build_responses_text_input(value, input_as_list);
            }
            let mut content = vec![json!({ "type": "input_text", "text": value })];
            content.extend(images.iter().map(|image| json!({
                "type": "input_image",
                "image_url": format!("data:{};base64,{}", image.mime_type, BASE64.encode(&image.bytes)),
            })));
            json!([{ "type": "message", "role": "user", "content": content }])
        },
    )
    .await?;
    authentication
        .require_session_generation(owner, generation)
        .await?;
    let text = response.content.trim().to_owned();
    if text.is_empty() {
        return Err("文本节点没有返回内容。".into());
    }
    Ok(text)
}

async fn generate_workflow_image(
    authentication: &AuthenticationManager,
    manager: &super::MediaStudioManager,
    owner: &str,
    generation: u64,
    request: super::provider::ImageGenerationRequest,
    model: GatewayModelConfig,
    preview_token: &str,
) -> Result<
    (
        super::history::ImageHistoryItem,
        super::provider::ImageGenerationInput,
    ),
    String,
> {
    let runtime = super::provider::resolve_image_model(owner, &request.model_config_id, model)?;
    let wire = super::provider::make_provider_request(&runtime, &request)?;
    let response = manager
        .provider_client
        .post(wire.url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, wire.content_type)
        .bearer_auth(&runtime.api_key)
        .body(wire.body)
        .send()
        .await
        .map_err(|_| "无法连接图片模型服务。".to_owned())?;
    let status = response.status();
    let body = super::read_provider_response(response).await?;
    if !status.is_success() {
        return Err(format!("图片节点生成失败（HTTP {}）。", status.as_u16()));
    }
    let result = super::provider::parse_provider_response(&request, &runtime, &body)?;
    authentication
        .require_session_generation(owner, generation)
        .await?;
    let outcome =
        super::persist_generation_result(manager, owner, &request.prompt, result, preview_token)
            .await;
    let item = match outcome {
        super::history::ImageGenerationOutcome::Saved { item } => item,
        super::history::ImageGenerationOutcome::GeneratedWithoutPersistence { .. } => {
            return Err("图片已生成但保存失败，工作流已停止。".into());
        }
    };
    let asset = item
        .images
        .first()
        .cloned()
        .ok_or_else(|| "图片节点没有返回结果。".to_owned())?;
    let (mime_type, bytes) = manager.history.read_history_asset(
        &super::history::owner_hash(owner),
        &item.id,
        &asset.content_token,
    )?;
    Ok((
        item,
        super::provider::ImageGenerationInput {
            name: format!("workflow-{}.png", asset.id),
            mime_type,
            bytes,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn text_model() -> GatewayModelConfig {
        GatewayModelConfig {
            id: "text-model".into(),
            owner_user_id: "alice".into(),
            name: "Text".into(),
            provider: "openai".into(),
            model: "gpt-6.1-sol".into(),
            api_key: Some("test-key".into()),
            base_url: Some("https://relay.example/v1/responses".into()),
            task_thinking_level: Some("medium".into()),
            temperature: None,
            max_output_tokens: Some(2_000),
            model_capability_revision: Some("capability-v1".into()),
            context_window_tokens: Some(128_000),
            output_reserve_tokens: Some(8_000),
            protocol_overhead_estimator_id: Some("responses-json-v1".into()),
            tokenizer_id: Some("o200k_base".into()),
            responses_compaction_threshold: None,
            enabled: true,
            task_enabled: Some(true),
            has_api_key: true,
            supports_images: true,
            supports_reasoning: true,
            supports_responses: true,
        }
    }

    fn workflow() -> MediaWorkflow {
        MediaWorkflow {
            id: Uuid::new_v4().to_string(),
            name: "demo".into(),
            input_prompt: "input".into(),
            input_images: vec![],
            nodes: vec![MediaWorkflowNode {
                id: Uuid::new_v4().to_string(),
                kind: MediaWorkflowNodeKind::Image,
                model_config_id: "image".into(),
                prompt: "draw".into(),
                reference_images: vec![],
                image_size: default_image_size(),
                video_size: default_video_size(),
                video_ratio: default_video_ratio(),
                video_seconds: default_video_seconds(),
            }],
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        }
    }

    fn model_for(id: &str, model_name: &str) -> GatewayModelConfig {
        let mut model = text_model();
        model.id = id.into();
        model.name = model_name.into();
        model.model = model_name.into();
        model.base_url = Some("https://provider.example/v1".into());
        model
    }

    fn node(kind: MediaWorkflowNodeKind, model_config_id: &str) -> MediaWorkflowNode {
        MediaWorkflowNode {
            id: Uuid::new_v4().to_string(),
            kind,
            model_config_id: model_config_id.into(),
            prompt: "run".into(),
            reference_images: vec![],
            image_size: default_image_size(),
            video_size: default_video_size(),
            video_ratio: default_video_ratio(),
            video_seconds: default_video_seconds(),
        }
    }

    #[test]
    fn validates_video_is_last_and_unique() {
        let mut value = workflow();
        value.nodes.push(MediaWorkflowNode {
            id: Uuid::new_v4().to_string(),
            kind: MediaWorkflowNodeKind::Video,
            model_config_id: "video".into(),
            prompt: "animate".into(),
            reference_images: vec![],
            image_size: default_image_size(),
            video_size: default_video_size(),
            video_ratio: default_video_ratio(),
            video_seconds: 4,
        });
        assert!(validate_workflow(&value).is_ok());
        value.nodes.swap(0, 1);
        assert!(validate_workflow(&value).is_err());
    }

    #[test]
    fn validates_minimax_h3_workflow_video_options() {
        let mut value = workflow();
        value.nodes = vec![node(MediaWorkflowNodeKind::Video, "video")];
        assert!(validate_workflow(&value).is_ok());

        value.nodes[0].video_size = "1280x720".into();
        assert_eq!(
            validate_workflow(&value).unwrap_err(),
            "视频节点尺寸、比例或时长无效。"
        );
        value.nodes[0].video_size = default_video_size();
        value.nodes[0].video_ratio = "2:1".into();
        assert!(validate_workflow(&value).is_err());
        value.nodes[0].video_ratio = default_video_ratio();
        value.nodes[0].video_seconds = 16;
        assert!(validate_workflow(&value).is_err());
    }

    #[test]
    fn keeps_workflow_video_poll_interval_on_the_shared_runtime_contract() {
        assert_eq!(VIDEO_POLL_INTERVAL, std::time::Duration::from_secs(10));
    }

    #[test]
    fn validates_workflow_text_capability_and_output_budget() {
        let model = text_model();
        let capability = validate_workflow_text_capability(&model).unwrap();
        assert_eq!(capability.0, "capability-v1");
        assert_eq!(capability.1, 128_000);
        assert_eq!(capability.2, 8_000);
        assert_eq!(capability.3, "responses-json-v1");

        let mut missing_revision = model.clone();
        missing_revision.model_capability_revision = Some("  ".into());
        assert!(validate_workflow_text_capability(&missing_revision).is_err());

        let mut invalid_budget = model;
        invalid_budget.max_output_tokens = Some(8_001);
        assert!(validate_workflow_text_capability(&invalid_budget).is_err());
    }

    #[test]
    fn rejects_non_positive_or_overlapping_workflow_text_capability() {
        let mut model = text_model();
        model.context_window_tokens = Some(0);
        assert!(validate_workflow_text_capability(&model).is_err());

        let mut model = text_model();
        model.output_reserve_tokens = model.context_window_tokens;
        assert!(validate_workflow_text_capability(&model).is_err());

        let mut model = text_model();
        model.max_output_tokens = Some(0);
        assert!(validate_workflow_text_capability(&model).is_err());
    }

    #[test]
    fn validates_every_workflow_model_before_execution() {
        let mut value = workflow();
        value.nodes.push(node(MediaWorkflowNodeKind::Text, "text"));
        value
            .nodes
            .push(node(MediaWorkflowNodeKind::Video, "video"));
        let models = HashMap::from([
            ("image".into(), model_for("image", "gpt-image-1")),
            ("text".into(), model_for("text", "gpt-6.1-sol")),
            ("video".into(), model_for("video", "MiniMax-H3")),
        ]);
        assert!(validate_workflow_models("alice", &value, &models).is_ok());

        let mut wrong_image = value.clone();
        wrong_image.nodes[0].model_config_id = "chat-as-image".into();
        let mut wrong_models = models.clone();
        wrong_models.insert(
            "chat-as-image".into(),
            model_for("chat-as-image", "gpt-6.1-sol"),
        );
        assert_eq!(
            validate_workflow_models("alice", &wrong_image, &wrong_models).unwrap_err(),
            "所选模型不支持图片生成。"
        );

        let mut no_vision = models;
        no_vision.get_mut("text").unwrap().supports_images = false;
        assert_eq!(
            validate_workflow_models("alice", &value, &no_vision).unwrap_err(),
            "文本节点所选模型不支持读取当前图片输入。"
        );
    }

    #[test]
    fn persists_lists_and_deletes_account_scoped_workflows() {
        let root = tempdir().unwrap();
        let store = MediaStudioWorkflowStore::new(root.path().to_path_buf()).unwrap();
        let value = workflow();
        assert!(store.save("alice", &value).unwrap());
        assert_eq!(store.list("alice").unwrap(), vec![value.clone()]);
        assert!(store.list("bob").unwrap().is_empty());
        store.delete("alice", &value.id).unwrap();
        assert!(store.list("alice").unwrap().is_empty());
    }

    #[test]
    fn loads_legacy_workflow_video_default_as_minimax_h3_768p() {
        let root = tempdir().unwrap();
        let store = MediaStudioWorkflowStore::new(root.path().to_path_buf()).unwrap();
        let mut value = workflow();
        value
            .nodes
            .push(node(MediaWorkflowNodeKind::Video, "video"));
        value.nodes[0].video_size = "1280x720".into();
        value.nodes[1].video_size = "1280x720".into();
        let folder = store.owner_directory("alice").join(&value.id);
        fs::create_dir_all(folder.join("assets")).unwrap();
        fs::write(
            folder.join("workflow.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();

        let loaded = store.list("alice").unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0]
            .nodes
            .iter()
            .all(|node| node.video_size == "768P" && node.video_seconds == 4));
    }

    #[test]
    fn rejects_stale_workflow_saves() {
        let root = tempdir().unwrap();
        let store = MediaStudioWorkflowStore::new(root.path().to_path_buf()).unwrap();
        let mut current = workflow();
        current.updated_at_unix_ms = 20;
        assert!(store.save("alice", &current).unwrap());

        let mut stale = current.clone();
        stale.name = "stale".into();
        stale.updated_at_unix_ms = 19;
        assert!(!store.save("alice", &stale).unwrap());
        assert_eq!(store.list("alice").unwrap(), vec![current]);
    }

    #[test]
    fn preserves_assets_created_after_the_previous_manifest_until_a_later_save() {
        let root = tempdir().unwrap();
        let store = MediaStudioWorkflowStore::new(root.path().to_path_buf()).unwrap();
        let mut value = workflow();
        assert!(store.save("alice", &value).unwrap());
        std::thread::sleep(std::time::Duration::from_millis(20));

        let assets = store
            .owner_directory("alice")
            .join(&value.id)
            .join("assets");
        let concurrent_asset = assets.join("concurrent.png");
        fs::write(&concurrent_asset, b"new asset").unwrap();
        value.updated_at_unix_ms += 1;
        assert!(store.save("alice", &value).unwrap());
        assert!(concurrent_asset.exists());

        std::thread::sleep(std::time::Duration::from_millis(20));
        value.updated_at_unix_ms += 1;
        assert!(store.save("alice", &value).unwrap());
        assert!(!concurrent_asset.exists());
    }
}
