use crate::{
    app_owned_export,
    authentication::AuthenticationManager,
    host_lifecycle::{validate_owner, HostManager},
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePriority,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_local_agent_protocol::{
    GetArtifactDataCommand, GetArtifactFileCommand, HostCommand, HostResult, ListArtifactsCommand,
    LocalAgentArtifact, LOCAL_AGENT_ARTIFACT_MAX_BYTES, LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use tauri::State;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedAgentArtifact {
    artifact_id: String,
    name: String,
    mime_type: String,
    size: u64,
    sha256: String,
    status: &'static str,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedAgentArtifactPage {
    artifacts: Vec<PresentedAgentArtifact>,
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedAgentArtifactPreview {
    artifact_id: String,
    markdown: String,
}

#[tauri::command]
pub async fn list_agent_artifacts(
    manager: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    owner_user_id: String,
    limit: u32,
    cursor: Option<String>,
) -> Result<PresentedAgentArtifactPage, String> {
    validate_owner(&owner_user_id)?;
    if !(1..=LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE).contains(&limit) {
        return Err(format!(
            "Agent 文档分页大小必须在 1 到 {LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE} 之间。"
        ));
    }
    if cursor.as_ref().is_some_and(|value| {
        value.is_empty() || value.len() > 300 || value.chars().any(char::is_control)
    }) {
        return Err("Agent 文档分页游标无效。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let result = manager
        .execute(HostCommand::ListArtifacts(ListArtifactsCommand {
            owner_user_id: owner_user_id.clone(),
            limit,
            cursor,
        }))
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let HostResult::Artifacts { page } = result else {
        return Err("Local Agent Host 返回了错误的文档列表响应。".into());
    };
    if page.artifacts.len() > limit as usize {
        return Err("Local Agent Host 返回的文档数量超过了请求上限。".into());
    }
    let artifacts = page
        .artifacts
        .into_iter()
        .map(|artifact| present_artifact(&owner_user_id, artifact))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PresentedAgentArtifactPage {
        artifacts,
        next_cursor: page.next_cursor,
    })
}

#[tauri::command]
pub async fn preview_agent_artifact(
    manager: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    owner_user_id: String,
    artifact_id: String,
    expected_size: u64,
    expected_sha256: String,
) -> Result<PresentedAgentArtifactPreview, String> {
    let generation = authenticated_artifact_request(
        &authentication,
        &owner_user_id,
        &artifact_id,
        expected_size,
        &expected_sha256,
    )
    .await?;
    let bytes = validated_artifact_bytes(
        &manager,
        &owner_user_id,
        &artifact_id,
        expected_size,
        &expected_sha256,
    )
    .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let markdown = String::from_utf8(bytes)
        .map_err(|_| "本地 Agent 文档不是有效的 UTF-8 Markdown。".to_string())?;
    Ok(PresentedAgentArtifactPreview {
        artifact_id,
        markdown,
    })
}

#[tauri::command]
pub async fn save_agent_artifact(
    manager: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    resources: State<'_, MachineResourceGovernor>,
    owner_user_id: String,
    artifact_id: String,
    name: String,
    expected_size: u64,
    expected_sha256: String,
) -> Result<bool, String> {
    let file_name = validate_file_name(&name)?;
    let generation = authenticated_artifact_request(
        &authentication,
        &owner_user_id,
        &artifact_id,
        expected_size,
        &expected_sha256,
    )
    .await?;
    let bytes = validated_artifact_bytes(
        &manager,
        &owner_user_id,
        &artifact_id,
        expected_size,
        &expected_sha256,
    )
    .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title("另存 Agent 文档")
        .set_file_name(&file_name)
        .add_filter("Markdown", &["md", "markdown"])
        .save_file()
        .await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let Some(selected) = selected else {
        return Ok(false);
    };
    let _permit = resources
        .acquire(
            MachineResourceKind::CodeReadWorker,
            MachineResourcePriority::Interactive,
        )
        .await?;
    let destination = selected.path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        app_owned_export::export_bytes(&destination, ".chatos-artifact-export-", &bytes)
    })
    .await
    .map_err(|_| "保存 Agent 文档任务异常退出。".to_string())??;
    Ok(true)
}

async fn authenticated_artifact_request(
    authentication: &AuthenticationManager,
    owner_user_id: &str,
    artifact_id: &str,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<u64, String> {
    validate_owner(owner_user_id)?;
    validate_artifact_identity(artifact_id)?;
    validate_integrity(expected_size, expected_sha256)?;
    authentication
        .session_generation_for_owner(owner_user_id)
        .await
}

async fn validated_artifact_bytes(
    manager: &HostManager,
    owner_user_id: &str,
    artifact_id: &str,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<Vec<u8>, String> {
    let file_result = manager
        .execute(HostCommand::GetArtifactFile(GetArtifactFileCommand {
            owner_user_id: owner_user_id.to_owned(),
            artifact_id: artifact_id.to_owned(),
        }))
        .await?;
    let HostResult::ArtifactFile { file } = file_result else {
        return Err("Local Agent Host 返回了错误的文档文件响应。".into());
    };
    if file.artifact_id != artifact_id
        || file.relative_path != format!("{artifact_id}.artifact")
        || file.relative_path.contains('/')
        || file.relative_path.contains('\\')
        || file.size != expected_size
        || !file.sha256.eq_ignore_ascii_case(expected_sha256)
    {
        return Err(integrity_error());
    }
    validate_integrity(file.size, &file.sha256)?;

    let data_result = manager
        .execute(HostCommand::GetArtifactData(GetArtifactDataCommand {
            owner_user_id: owner_user_id.to_owned(),
            artifact_id: artifact_id.to_owned(),
        }))
        .await?;
    let HostResult::ArtifactData {
        artifact_id: returned_id,
        data_base64,
    } = data_result
    else {
        return Err("Local Agent Host 返回了错误的文档内容响应。".into());
    };
    if returned_id != artifact_id {
        return Err(integrity_error());
    }
    let bytes = STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|_| "Local Agent Host 返回了无效的文档内容。".to_string())?;
    if bytes.len() as u64 != expected_size
        || hex_sha256(&bytes) != expected_sha256.to_ascii_lowercase()
    {
        return Err(integrity_error());
    }
    Ok(bytes)
}

fn present_artifact(
    owner_user_id: &str,
    artifact: LocalAgentArtifact,
) -> Result<PresentedAgentArtifact, String> {
    if artifact.owner_user_id != owner_user_id {
        return Err("Local Agent Host 返回了其他账号的文档。".into());
    }
    validate_artifact_identity(&artifact.artifact_id)?;
    if artifact.name.trim().is_empty()
        || artifact.name.len() > 1_000
        || artifact.name.chars().any(char::is_control)
    {
        return Err("Local Agent Host 返回了无效的文档名称。".into());
    }
    if artifact.mime_type.trim().is_empty()
        || artifact.mime_type.len() > 255
        || artifact.mime_type.chars().any(char::is_control)
    {
        return Err("Local Agent Host 返回了无效的文档类型。".into());
    }
    validate_integrity(artifact.size, &artifact.sha256)?;
    Ok(PresentedAgentArtifact {
        artifact_id: artifact.artifact_id,
        name: artifact.name,
        mime_type: artifact.mime_type,
        size: artifact.size,
        sha256: artifact.sha256.to_ascii_lowercase(),
        status: "stored",
        created_at_unix_ms: artifact.created_at_unix_ms,
        updated_at_unix_ms: artifact.updated_at_unix_ms,
    })
}

fn validate_artifact_identity(value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > 512
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || value.contains('/')
        || value.contains('\\')
    {
        return Err("artifactId is invalid".into());
    }
    Ok(())
}

fn validate_integrity(size: u64, sha256: &str) -> Result<(), String> {
    if size == 0 || size > LOCAL_AGENT_ARTIFACT_MAX_BYTES as u64 {
        return Err(integrity_error());
    }
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(integrity_error());
    }
    Ok(())
}

fn validate_file_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 255
        || trimmed.chars().any(char::is_control)
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || Path::new(trimmed)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(trimmed)
    {
        return Err("Agent 文档文件名无效。".into());
    }
    Ok(trimmed.to_owned())
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn integrity_error() -> String {
    "本地 Agent 文档的大小或 SHA-256 与元数据不一致。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_artifact_projection_and_normalizes_digest() {
        let artifact = LocalAgentArtifact {
            artifact_id: "artifact-1".into(),
            owner_user_id: "owner-1".into(),
            name: "report.md".into(),
            mime_type: "text/markdown".into(),
            size: 4,
            sha256: "A".repeat(64),
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
        };
        let presented = present_artifact("owner-1", artifact).expect("valid artifact");
        assert_eq!(presented.sha256, "a".repeat(64));
        assert_eq!(presented.status, "stored");
    }

    #[test]
    fn rejects_cross_account_and_unsafe_artifact_metadata() {
        let artifact = LocalAgentArtifact {
            artifact_id: "../escape".into(),
            owner_user_id: "another-owner".into(),
            name: "report.md".into(),
            mime_type: "text/markdown".into(),
            size: 4,
            sha256: "a".repeat(64),
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
        };
        assert!(present_artifact("owner-1", artifact.clone()).is_err());
        assert!(present_artifact("another-owner", artifact).is_err());
        assert!(validate_file_name("../report.md").is_err());
        assert!(validate_file_name("report.md").is_ok());
    }

    #[test]
    fn hashes_content_with_lowercase_sha256() {
        assert_eq!(
            hex_sha256(b"test"),
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }
}
