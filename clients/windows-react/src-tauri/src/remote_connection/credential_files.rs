use super::{
    models::{RemoteCredentialMutation, RemoteSecretMutation},
    ssh::is_reparse_point,
    RemoteConnectionManager,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex as StandardMutex,
    time::{Duration, Instant},
};
use tauri::State;

const SELECTION_LIFETIME: Duration = Duration::from_secs(15 * 60);
const MAXIMUM_PENDING_SELECTIONS: usize = 32;
const MAXIMUM_CREDENTIAL_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCredentialFileKind {
    PrivateKey,
    Certificate,
    JumpPrivateKey,
    JumpCertificate,
}

#[derive(Clone, Debug)]
struct PendingCredentialFileSelection {
    owner_user_id: String,
    presentation_id: String,
    kind: RemoteCredentialFileKind,
    path: PathBuf,
    created_at: Instant,
}

#[derive(Default)]
pub(super) struct RemoteCredentialFileSelectionStore {
    selections: StandardMutex<HashMap<String, PendingCredentialFileSelection>>,
    closed_presentations: StandardMutex<HashMap<String, Instant>>,
}

impl RemoteCredentialFileSelectionStore {
    pub(super) fn stage(
        &self,
        owner_user_id: &str,
        presentation_id: &str,
        kind: RemoteCredentialFileKind,
        path: PathBuf,
        file_name: String,
    ) -> Result<PresentedRemoteCredentialFileSelection, String> {
        validate_presentation_id(presentation_id)?;
        let key = presentation_key(owner_user_id, presentation_id);
        let mut closed = self
            .closed_presentations
            .lock()
            .map_err(|_| selection_error())?;
        closed.retain(|_, closed_at| closed_at.elapsed() <= SELECTION_LIFETIME);
        if closed.contains_key(&key) {
            return Err(selection_expired());
        }
        drop(closed);

        let mut selections = self.selections.lock().map_err(|_| selection_error())?;
        selections.retain(|_, selection| selection.created_at.elapsed() <= SELECTION_LIFETIME);
        if selections.len() >= MAXIMUM_PENDING_SELECTIONS {
            return Err("待处理的 SSH 凭据文件过多，请关闭其他连接编辑窗口后重试。".into());
        }
        let selection_id = format!("remote-credential-file-{}", uuid::Uuid::new_v4());
        selections.insert(
            selection_id.clone(),
            PendingCredentialFileSelection {
                owner_user_id: owner_user_id.to_owned(),
                presentation_id: presentation_id.to_owned(),
                kind,
                path,
                created_at: Instant::now(),
            },
        );
        Ok(PresentedRemoteCredentialFileSelection {
            selection_id,
            file_name,
            kind,
        })
    }

    fn resolve_path(
        &self,
        owner_user_id: &str,
        presentation_id: Option<&str>,
        selection_id: &str,
        kind: RemoteCredentialFileKind,
    ) -> Result<String, String> {
        let presentation_id = presentation_id.ok_or_else(selection_expired)?;
        validate_presentation_id(presentation_id)?;
        validate_selection_id(selection_id)?;
        let mut selections = self.selections.lock().map_err(|_| selection_error())?;
        selections.retain(|_, selection| selection.created_at.elapsed() <= SELECTION_LIFETIME);
        let selection = selections
            .get(selection_id)
            .filter(|selection| {
                selection.owner_user_id == owner_user_id
                    && selection.presentation_id == presentation_id
                    && selection.kind == kind
            })
            .ok_or_else(selection_expired)?;
        selection
            .path
            .to_str()
            .map(str::to_owned)
            .ok_or_else(selection_error)
    }

    pub(super) fn resolve_mutation(
        &self,
        owner_user_id: &str,
        presentation_id: Option<&str>,
        mutation: &RemoteCredentialMutation,
    ) -> Result<RemoteCredentialMutation, String> {
        Ok(RemoteCredentialMutation {
            password: mutation.password.clone(),
            private_key_path: self.resolve_file_mutation(
                owner_user_id,
                presentation_id,
                &mutation.private_key_path,
                RemoteCredentialFileKind::PrivateKey,
            )?,
            certificate_path: self.resolve_file_mutation(
                owner_user_id,
                presentation_id,
                &mutation.certificate_path,
                RemoteCredentialFileKind::Certificate,
            )?,
            jump_password: mutation.jump_password.clone(),
            jump_private_key_path: self.resolve_file_mutation(
                owner_user_id,
                presentation_id,
                &mutation.jump_private_key_path,
                RemoteCredentialFileKind::JumpPrivateKey,
            )?,
            jump_certificate_path: self.resolve_file_mutation(
                owner_user_id,
                presentation_id,
                &mutation.jump_certificate_path,
                RemoteCredentialFileKind::JumpCertificate,
            )?,
        })
    }

    fn resolve_file_mutation(
        &self,
        owner_user_id: &str,
        presentation_id: Option<&str>,
        mutation: &RemoteSecretMutation,
        kind: RemoteCredentialFileKind,
    ) -> Result<RemoteSecretMutation, String> {
        match mutation {
            RemoteSecretMutation::Keep => Ok(RemoteSecretMutation::Keep),
            RemoteSecretMutation::Clear => Ok(RemoteSecretMutation::Clear),
            RemoteSecretMutation::Replace(selection_id) => self
                .resolve_path(owner_user_id, presentation_id, selection_id, kind)
                .map(RemoteSecretMutation::Replace),
        }
    }

    pub(super) fn discard(&self, owner_user_id: &str, presentation_id: &str) {
        if let Ok(mut closed) = self.closed_presentations.lock() {
            closed.retain(|_, closed_at| closed_at.elapsed() <= SELECTION_LIFETIME);
            closed.insert(
                presentation_key(owner_user_id, presentation_id),
                Instant::now(),
            );
        }
        if let Ok(mut selections) = self.selections.lock() {
            selections.retain(|_, selection| {
                selection.owner_user_id != owner_user_id
                    || selection.presentation_id != presentation_id
            });
        }
    }

    pub(super) fn reset(&self) {
        if let Ok(mut selections) = self.selections.lock() {
            selections.clear();
        }
        if let Ok(mut closed) = self.closed_presentations.lock() {
            closed.clear();
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedRemoteCredentialFileSelection {
    pub(super) selection_id: String,
    file_name: String,
    kind: RemoteCredentialFileKind,
}

#[tauri::command]
pub async fn pick_remote_credential_file(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
    kind: RemoteCredentialFileKind,
) -> Result<Option<PresentedRemoteCredentialFileSelection>, String> {
    crate::host_lifecycle::validate_owner(&owner_user_id)?;
    validate_presentation_id(&presentation_id)?;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title(selection_title(kind))
        .pick_file()
        .await;
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let requested = selected.path().to_path_buf();
    let (path, file_name) =
        tauri::async_runtime::spawn_blocking(move || prepare_credential_file(&requested))
            .await
            .map_err(|_| selection_error())??;
    manager
        .credential_file_selections
        .stage(&owner_user_id, &presentation_id, kind, path, file_name)
        .map(Some)
}

#[tauri::command]
pub async fn discard_remote_credential_file_selections(
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    presentation_id: String,
) -> Result<(), String> {
    crate::host_lifecycle::validate_owner(&owner_user_id)?;
    validate_presentation_id(&presentation_id)?;
    manager
        .credential_file_selections
        .discard(&owner_user_id, &presentation_id);
    Ok(())
}

fn prepare_credential_file(requested: &Path) -> Result<(PathBuf, String), String> {
    let requested_metadata = fs::symlink_metadata(requested).map_err(|_| selection_error())?;
    validate_credential_file(&requested_metadata)?;
    let canonical = fs::canonicalize(requested).map_err(|_| selection_error())?;
    let canonical_metadata = fs::symlink_metadata(&canonical).map_err(|_| selection_error())?;
    validate_credential_file(&canonical_metadata)?;
    let file_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty() && value.len() <= 1_024 && !value.chars().any(char::is_control)
        })
        .ok_or_else(selection_error)?
        .to_owned();
    Ok((canonical, file_name))
}

fn validate_credential_file(metadata: &fs::Metadata) -> Result<(), String> {
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(metadata)
        || metadata.len() > MAXIMUM_CREDENTIAL_FILE_BYTES
    {
        return Err(selection_error());
    }
    Ok(())
}

fn selection_title(kind: RemoteCredentialFileKind) -> &'static str {
    match kind {
        RemoteCredentialFileKind::PrivateKey => "选择 SSH 私钥文件",
        RemoteCredentialFileKind::Certificate => "选择 SSH 证书文件",
        RemoteCredentialFileKind::JumpPrivateKey => "选择跳板机 SSH 私钥文件",
        RemoteCredentialFileKind::JumpCertificate => "选择跳板机 SSH 证书文件",
    }
}

pub(super) fn validate_presentation_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 1_000
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(selection_expired());
    }
    Ok(())
}

fn validate_selection_id(value: &str) -> Result<(), String> {
    if !value.starts_with("remote-credential-file-")
        || value.len() > 1_000
        || value.chars().any(char::is_control)
    {
        return Err(selection_expired());
    }
    Ok(())
}

fn presentation_key(owner_user_id: &str, presentation_id: &str) -> String {
    format!("{owner_user_id}\0{presentation_id}")
}

fn selection_expired() -> String {
    "SSH 凭据文件选择已失效，请重新选择。".into()
}

fn selection_error() -> String {
    "无法使用所选 SSH 凭据文件。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_selection_is_owner_presentation_and_kind_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("id_ed25519");
        fs::write(&path, b"key").unwrap();
        let store = RemoteCredentialFileSelectionStore::default();
        let selected = store
            .stage(
                "owner-a",
                "sheet-a",
                RemoteCredentialFileKind::PrivateKey,
                path.clone(),
                "id_ed25519".into(),
            )
            .unwrap();

        for (owner, presentation, kind) in [
            ("owner-b", "sheet-a", RemoteCredentialFileKind::PrivateKey),
            ("owner-a", "sheet-b", RemoteCredentialFileKind::PrivateKey),
            ("owner-a", "sheet-a", RemoteCredentialFileKind::Certificate),
        ] {
            assert!(store
                .resolve_path(owner, Some(presentation), &selected.selection_id, kind,)
                .is_err());
        }
        assert_eq!(
            store
                .resolve_path(
                    "owner-a",
                    Some("sheet-a"),
                    &selected.selection_id,
                    RemoteCredentialFileKind::PrivateKey,
                )
                .unwrap(),
            path.to_string_lossy()
        );
        let serialized = serde_json::to_string(&selected).unwrap();
        assert!(!serialized.contains(directory.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn closing_a_presentation_rejects_existing_and_late_selections() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("id_ed25519");
        fs::write(&path, b"key").unwrap();
        let store = RemoteCredentialFileSelectionStore::default();
        let selected = store
            .stage(
                "owner-a",
                "sheet-a",
                RemoteCredentialFileKind::PrivateKey,
                path.clone(),
                "id_ed25519".into(),
            )
            .unwrap();
        store.discard("owner-a", "sheet-a");

        assert!(store
            .resolve_path(
                "owner-a",
                Some("sheet-a"),
                &selected.selection_id,
                RemoteCredentialFileKind::PrivateKey,
            )
            .is_err());
        assert!(store
            .stage(
                "owner-a",
                "sheet-a",
                RemoteCredentialFileKind::PrivateKey,
                path,
                "id_ed25519".into(),
            )
            .is_err());
    }

    #[test]
    fn credential_file_validation_rejects_oversized_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("too-large.key");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAXIMUM_CREDENTIAL_FILE_BYTES + 1).unwrap();
        assert!(prepare_credential_file(&path).is_err());
    }
}
