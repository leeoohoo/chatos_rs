use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use super::video_provider::VideoInput;

const MAXIMUM_AUDIO_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_CLOSED_PRESENTATIONS: usize = 64;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaVideoAudioDraft {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub byte_size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct InputScope {
    owner_hash: String,
    authentication_generation: u64,
    presentation_id: String,
    presentation_hash: String,
}

#[derive(Clone)]
struct AudioEntry {
    scope: InputScope,
    id: String,
    name: String,
    mime_type: String,
    byte_size: u64,
    sha256: String,
    file: PathBuf,
}

impl AudioEntry {
    fn draft(&self) -> MediaVideoAudioDraft {
        MediaVideoAudioDraft {
            id: self.id.clone(),
            name: self.name.clone(),
            mime_type: self.mime_type.clone(),
            byte_size: self.byte_size,
        }
    }
}

#[derive(Default)]
struct InputState {
    epoch: u64,
    entries: HashMap<String, AudioEntry>,
    closed_presentations: HashSet<InputScope>,
}

struct VideoInputVaultInner {
    root: PathBuf,
    state: Mutex<InputState>,
}

#[derive(Clone)]
pub(super) struct MediaVideoInputVault {
    inner: Arc<VideoInputVaultInner>,
}

impl MediaVideoInputVault {
    pub(super) fn new(root: PathBuf) -> Result<Self, String> {
        if root.exists() {
            let metadata = fs::symlink_metadata(&root).map_err(|_| input_error())?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(input_error());
            }
            fs::remove_dir_all(&root).map_err(|_| input_error())?;
        }
        ensure_private_directory(&root)?;
        let root = fs::canonicalize(root).map_err(|_| input_error())?;
        Ok(Self {
            inner: Arc::new(VideoInputVaultInner {
                root,
                state: Mutex::new(InputState::default()),
            }),
        })
    }

    pub(super) fn begin_scope(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(u64, String), String> {
        let scope = input_scope(owner_user_id, authentication_generation, presentation_id)?;
        let state = self.state()?;
        if state.closed_presentations.contains(&scope) {
            return Err("视频创作页面已关闭，请重新打开后选择参考音频。".into());
        }
        Ok((state.epoch, scope.presentation_id))
    }

    pub(super) fn stage_path(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        epoch: u64,
        path: PathBuf,
    ) -> Result<MediaVideoAudioDraft, String> {
        let scope = input_scope(owner_user_id, authentication_generation, presentation_id)?;
        let candidate = read_audio_candidate(&path)?;
        let mut state = self.state()?;
        if state.epoch != epoch || state.closed_presentations.contains(&scope) {
            return Err("参考音频选择已失效，请重新选择。".into());
        }
        let scope_directory = self
            .inner
            .root
            .join(&scope.owner_hash)
            .join(scope.authentication_generation.to_string())
            .join(&scope.presentation_hash);
        ensure_private_directory(&scope_directory)?;
        let id = format!("video_audio_{}", Uuid::new_v4().simple());
        let file = scope_directory.join(format!("{}.bin", Uuid::new_v4().simple()));
        write_private_file(&file, &candidate.bytes)?;
        let entry = AudioEntry {
            scope: scope.clone(),
            id: id.clone(),
            name: candidate.name,
            mime_type: candidate.mime_type,
            byte_size: candidate.bytes.len() as u64,
            sha256: sha256(&candidate.bytes),
            file,
        };
        // Only one audio draft is live per presentation. Replacing it is atomic from the
        // consumer's perspective and removes the superseded native file.
        let replaced = state
            .entries
            .iter()
            .find(|(_, value)| value.scope == scope)
            .map(|(key, _)| key.clone())
            .and_then(|key| state.entries.remove(&key));
        let draft = entry.draft();
        state.entries.insert(id, entry);
        drop(state);
        if let Some(replaced) = replaced {
            let _ = fs::remove_file(replaced.file);
        }
        Ok(draft)
    }

    pub(super) fn resolve(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        audio_id: &str,
    ) -> Result<VideoInput, String> {
        if !valid_audio_id(audio_id) {
            return Err("参考音频选择无效，请重新选择。".into());
        }
        let scope = input_scope(owner_user_id, authentication_generation, presentation_id)?;
        let entry = {
            let state = self.state()?;
            if state.closed_presentations.contains(&scope) {
                return Err("视频创作页面已关闭，请重新打开后选择参考音频。".into());
            }
            state
                .entries
                .get(audio_id)
                .filter(|entry| entry.scope == scope)
                .cloned()
                .ok_or_else(|| "参考音频已失效，请重新选择。".to_owned())?
        };
        let bytes = read_verified_entry(&entry)?;
        Ok(VideoInput {
            mime_type: entry.mime_type,
            bytes,
        })
    }

    pub(super) fn discard(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        audio_id: &str,
    ) -> Result<(), String> {
        if !valid_audio_id(audio_id) {
            return Err("参考音频选择无效，请重新选择。".into());
        }
        let scope = input_scope(owner_user_id, authentication_generation, presentation_id)?;
        let removed = {
            let mut state = self.state()?;
            if state
                .entries
                .get(audio_id)
                .is_some_and(|entry| entry.scope == scope)
            {
                state.entries.remove(audio_id)
            } else {
                None
            }
        };
        if let Some(entry) = removed {
            let _ = fs::remove_file(entry.file);
        }
        Ok(())
    }

    pub(super) fn close_presentation(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(), String> {
        let scope = input_scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut removed = Vec::new();
        {
            let mut state = self.state()?;
            state.epoch = state.epoch.wrapping_add(1);
            state.closed_presentations.insert(scope.clone());
            if state.closed_presentations.len() > MAXIMUM_CLOSED_PRESENTATIONS {
                state.closed_presentations.clear();
                state.closed_presentations.insert(scope.clone());
            }
            let ids = state
                .entries
                .iter()
                .filter(|(_, entry)| entry.scope == scope)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                if let Some(entry) = state.entries.remove(&id) {
                    removed.push(entry);
                }
            }
        }
        for entry in removed {
            let _ = fs::remove_file(entry.file);
        }
        let directory = self
            .inner
            .root
            .join(&scope.owner_hash)
            .join(scope.authentication_generation.to_string())
            .join(&scope.presentation_hash);
        let _ = fs::remove_dir_all(directory);
        Ok(())
    }

    pub(super) fn reset(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.epoch = state.epoch.wrapping_add(1);
            state.entries.clear();
            state.closed_presentations.clear();
        }
        let _ = fs::remove_dir_all(&self.inner.root);
        let _ = ensure_private_directory(&self.inner.root);
    }

    fn state(&self) -> Result<std::sync::MutexGuard<'_, InputState>, String> {
        self.inner.state.lock().map_err(|_| input_error())
    }
}

struct AudioCandidate {
    name: String,
    mime_type: String,
    bytes: Vec<u8>,
}

fn read_audio_candidate(path: &Path) -> Result<AudioCandidate, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitized_name)
        .filter(|value| !value.is_empty())
        .ok_or_else(input_error)?;
    let bytes = read_private_file(path, MAXIMUM_AUDIO_BYTES)?;
    let mime_type = detected_audio_mime_type(path, &bytes).ok_or_else(|| {
        "参考音频必须是有效的 MP3、WAV、M4A 或 AAC，且文件不能超过 20 MB。".to_owned()
    })?;
    Ok(AudioCandidate {
        name,
        mime_type,
        bytes,
    })
}

fn detected_audio_mime_type(path: &Path, bytes: &[u8]) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "wav" && bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
    {
        return Some("audio/wav".into());
    }
    if extension == "mp3"
        && (bytes.starts_with(b"ID3")
            || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0))
    {
        return Some("audio/mpeg".into());
    }
    if matches!(extension.as_str(), "m4a" | "mp4") && bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return Some("audio/mp4".into());
    }
    if extension == "aac" && bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xf6 == 0xf0 {
        return Some("audio/aac".into());
    }
    None
}

fn read_verified_entry(entry: &AudioEntry) -> Result<Vec<u8>, String> {
    let bytes = read_private_file(&entry.file, MAXIMUM_AUDIO_BYTES)?;
    if bytes.len() as u64 != entry.byte_size || sha256(&bytes) != entry.sha256 {
        return Err(input_error());
    }
    Ok(bytes)
}

fn read_private_file(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| input_error())?;
    if !link_metadata.file_type().is_file()
        || link_metadata.file_type().is_symlink()
        || link_metadata.len() == 0
        || link_metadata.len() > maximum_bytes as u64
    {
        return Err(input_error());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path).map_err(|_| input_error())?;
    let before = file.metadata().map_err(|_| input_error())?;
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take((maximum_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| input_error())?;
    let after = file.metadata().map_err(|_| input_error())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
        || bytes.len() > maximum_bytes
    {
        return Err(input_error());
    }
    Ok(bytes)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| input_error())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| input_error())
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|_| input_error())?;
    let metadata = fs::symlink_metadata(path).map_err(|_| input_error())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(input_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| input_error())?;
    }
    Ok(())
}

fn input_scope(
    owner_user_id: &str,
    authentication_generation: u64,
    presentation_id: &str,
) -> Result<InputScope, String> {
    let parsed = Uuid::parse_str(presentation_id).map_err(|_| "视频创作页面标识无效。")?;
    if parsed.to_string() != presentation_id {
        return Err("视频创作页面标识无效。".into());
    }
    let presentation_id = parsed.to_string();
    Ok(InputScope {
        owner_hash: sha256(owner_user_id.as_bytes()),
        authentication_generation,
        presentation_hash: sha256(presentation_id.as_bytes()),
        presentation_id,
    })
}

fn valid_audio_id(value: &str) -> bool {
    value.strip_prefix("video_audio_").is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn sanitized_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() && *character != '/' && *character != '\\')
        .take(255)
        .collect::<String>()
}

fn sha256(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}

fn input_error() -> String {
    "参考音频无效或已失效，请重新选择。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn wav() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[4, 0, 0, 0]);
        bytes.extend_from_slice(b"WAVEdata");
        bytes
    }

    #[test]
    fn audio_token_is_owner_generation_and_presentation_scoped() {
        let root = tempdir().unwrap();
        let source = root.path().join("voice.wav");
        fs::write(&source, wav()).unwrap();
        let vault = MediaVideoInputVault::new(root.path().join("vault")).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let (epoch, _) = vault.begin_scope("alice", 7, &presentation).unwrap();
        let draft = vault
            .stage_path("alice", 7, &presentation, epoch, source)
            .unwrap();
        assert_eq!(draft.mime_type, "audio/wav");
        assert_eq!(
            vault
                .resolve("alice", 7, &presentation, &draft.id)
                .unwrap()
                .bytes,
            wav()
        );
        assert!(vault.resolve("bob", 7, &presentation, &draft.id).is_err());
        assert!(vault.resolve("alice", 8, &presentation, &draft.id).is_err());
        assert!(vault
            .resolve("alice", 7, &Uuid::new_v4().to_string(), &draft.id)
            .is_err());
    }

    #[test]
    fn replacement_and_discard_remove_native_material() {
        let root = tempdir().unwrap();
        let first = root.path().join("first.wav");
        let second = root.path().join("second.wav");
        fs::write(&first, wav()).unwrap();
        fs::write(&second, wav()).unwrap();
        let vault_root = root.path().join("vault");
        let vault = MediaVideoInputVault::new(vault_root.clone()).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let (epoch, _) = vault.begin_scope("alice", 1, &presentation).unwrap();
        let old = vault
            .stage_path("alice", 1, &presentation, epoch, first)
            .unwrap();
        let current = vault
            .stage_path("alice", 1, &presentation, epoch, second)
            .unwrap();
        assert!(vault.resolve("alice", 1, &presentation, &old.id).is_err());
        vault
            .discard("alice", 1, &presentation, &current.id)
            .unwrap();
        assert!(vault
            .resolve("alice", 1, &presentation, &current.id)
            .is_err());
        assert_eq!(
            fs::read_dir(vault_root)
                .unwrap()
                .flat_map(|entry| fs::read_dir(entry.unwrap().path()).unwrap())
                .flat_map(|entry| fs::read_dir(entry.unwrap().path()).unwrap())
                .flat_map(|entry| fs::read_dir(entry.unwrap().path()).unwrap())
                .count(),
            0
        );
    }

    #[test]
    fn closed_or_reset_scope_rejects_late_picker_results() {
        let root = tempdir().unwrap();
        let source = root.path().join("voice.wav");
        fs::write(&source, wav()).unwrap();
        let vault = MediaVideoInputVault::new(root.path().join("vault")).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let (epoch, _) = vault.begin_scope("alice", 1, &presentation).unwrap();
        vault.close_presentation("alice", 1, &presentation).unwrap();
        assert!(vault
            .stage_path("alice", 1, &presentation, epoch, source.clone())
            .is_err());

        let replacement = Uuid::new_v4().to_string();
        let (epoch, _) = vault.begin_scope("alice", 1, &replacement).unwrap();
        vault.reset();
        assert!(vault
            .stage_path("alice", 1, &replacement, epoch, source)
            .is_err());
    }

    #[test]
    fn extension_and_content_must_agree() {
        let root = tempdir().unwrap();
        let fake = root.path().join("voice.mp3");
        fs::write(&fake, wav()).unwrap();
        let vault = MediaVideoInputVault::new(root.path().join("vault")).unwrap();
        let presentation = Uuid::new_v4().to_string();
        let (epoch, _) = vault.begin_scope("alice", 1, &presentation).unwrap();
        assert!(vault
            .stage_path("alice", 1, &presentation, epoch, fake)
            .is_err());
    }
}
