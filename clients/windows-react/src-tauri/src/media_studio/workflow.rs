use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chatos_ai_runtime::simple_prompt::SimplePromptOptions;
use chatos_ai_runtime::{
    build_responses_text_input, run_compatible_prompt_with, AiRequestHandler, ModelRuntimeConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use tauri::State;
use uuid::Uuid;

use crate::{
    app_data_transaction::{ensure_owned_directory, replace_owned_file},
    authentication::AuthenticationManager,
    host_lifecycle::validate_owner,
};

const MAXIMUM_MANIFEST_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_TEXT_LENGTH: usize = 20_000;

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
    "1280x720".into()
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
            if node.video_size.trim().is_empty()
                || node.video_ratio.trim().is_empty()
                || !(1..=120).contains(&node.video_seconds)
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
}

impl MediaStudioWorkflowStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let root = app_data_directory.join("MediaStudio").join("Workflows");
        ensure_owned_directory(&root).map_err(|_| "无法创建工作流目录。".to_owned())?;
        Ok(Self { root })
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
            let Ok(workflow) = self.load(&path) else {
                continue;
            };
            workflows.push(workflow);
        }
        workflows.sort_by(|left, right| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms));
        Ok(workflows)
    }

    pub(crate) fn save(&self, owner: &str, workflow: &MediaWorkflow) -> Result<(), String> {
        validate_owner(owner)?;
        validate_workflow(workflow)?;
        let folder = self.workflow_directory(owner, &workflow.id)?;
        let assets = folder.join("assets");
        ensure_owned_directory(&assets).map_err(|_| "无法创建工作流资产目录。".to_owned())?;
        let bytes = serde_json::to_vec(workflow).map_err(|_| "无法编码工作流。".to_owned())?;
        if bytes.len() > MAXIMUM_MANIFEST_BYTES {
            return Err("工作流清单过大。".into());
        }
        replace_owned_file(&folder, &folder.join("workflow.json"), ".workflow-", &bytes)
            .map_err(|_| "无法保存工作流。".to_owned())?;
        self.cleanup_assets(&assets, workflow)?;
        Ok(())
    }

    pub(crate) fn delete(&self, owner: &str, id: &str) -> Result<(), String> {
        validate_owner(owner)?;
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

    fn load(&self, folder: &Path) -> Result<MediaWorkflow, String> {
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
        let workflow: MediaWorkflow =
            serde_json::from_slice(&bytes).map_err(|_| "工作流清单无效。".to_owned())?;
        if workflow.id
            != folder
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
        {
            return Err("工作流清单标识不匹配。".into());
        }
        validate_workflow(&workflow)?;
        Ok(workflow)
    }

    fn cleanup_assets(&self, assets: &Path, workflow: &MediaWorkflow) -> Result<(), String> {
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
) -> Result<(), String> {
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaWorkflowRunResult {
    pub workflow_id: String,
    pub client_request_id: String,
    pub status: String,
    pub final_text: String,
    pub image_record_ids: Vec<String>,
    pub error_message: Option<String>,
}

#[tauri::command]
pub(crate) async fn run_media_studio_workflow(
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
    let preview_token = manager.activate_session(&request.owner_user_id, generation)?;
    let mut current_text = workflow.input_prompt.trim().to_owned();
    let mut current_images = manager.workflows.resolve_images(
        &request.owner_user_id,
        &workflow.input_images,
        &workflow.id,
    )?;
    let mut image_record_ids = Vec::new();
    for node in &workflow.nodes {
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
        match node.kind {
            MediaWorkflowNodeKind::Text => {
                current_text = generate_workflow_text(
                    &authentication,
                    &manager,
                    &request.owner_user_id,
                    generation,
                    &node.model_config_id,
                    &prompt,
                    &current_images,
                    &references,
                )
                .await?;
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
                    &preview_token,
                )
                .await?;
                image_record_ids.push(item.id);
                current_images = vec![next_image];
            }
            MediaWorkflowNodeKind::Video => {
                return Err("Windows 工作流视频节点执行器尚未接入持久视频任务。".into());
            }
        }
    }
    Ok(MediaWorkflowRunResult {
        workflow_id: workflow.id,
        client_request_id: request.client_request_id,
        status: "completed".into(),
        final_text: current_text,
        image_record_ids,
        error_message: None,
    })
}

async fn generate_workflow_text(
    authentication: &AuthenticationManager,
    manager: &super::MediaStudioManager,
    owner: &str,
    generation: u64,
    model_id: &str,
    prompt: &str,
    current_images: &[super::provider::ImageGenerationInput],
    references: &[super::provider::ImageGenerationInput],
) -> Result<String, String> {
    let mut models = authentication
        .resolved_models_for_owner(owner, vec![model_id.to_owned()])
        .await
        .map_err(|error| error.message)?;
    let model = models
        .pop()
        .ok_or_else(|| "所选文本模型已不可用。".to_owned())?;
    if !model.enabled
        || !model.has_api_key
        || (!current_images.is_empty() && !model.supports_images)
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
    let config = ModelRuntimeConfig {
        model_capability_revision: model.model_capability_revision.clone().unwrap_or_default(),
        context_window_tokens: model.context_window_tokens.unwrap_or_default().max(0) as usize,
        output_reserve_tokens: model.output_reserve_tokens.unwrap_or_default().max(0) as usize,
        protocol_overhead_estimator_id: model
            .protocol_overhead_estimator_id
            .clone()
            .unwrap_or_default(),
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
        SimplePromptOptions { max_output_tokens: model.max_output_tokens, ..Default::default() },
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
    preview_token: &str,
) -> Result<
    (
        super::history::ImageHistoryItem,
        super::provider::ImageGenerationInput,
    ),
    String,
> {
    let mut models = authentication
        .resolved_models_for_owner(owner, vec![request.model_config_id.clone()])
        .await
        .map_err(|error| error.message)?;
    let model = models
        .pop()
        .ok_or_else(|| "所选图片模型已不可用。".to_owned())?;
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
    fn persists_lists_and_deletes_account_scoped_workflows() {
        let root = tempdir().unwrap();
        let store = MediaStudioWorkflowStore::new(root.path().to_path_buf()).unwrap();
        let value = workflow();
        store.save("alice", &value).unwrap();
        assert_eq!(store.list("alice").unwrap(), vec![value.clone()]);
        assert!(store.list("bob").unwrap().is_empty());
        store.delete("alice", &value.id).unwrap();
        assert!(store.list("alice").unwrap().is_empty());
    }
}
