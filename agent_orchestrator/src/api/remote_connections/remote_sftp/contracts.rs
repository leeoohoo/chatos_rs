use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(crate) struct SftpListQuery {
    pub(super) path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SftpUploadRequest {
    pub(super) local_path: Option<String>,
    pub(super) remote_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SftpDownloadRequest {
    pub(super) remote_path: Option<String>,
    pub(super) local_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SftpTransferStartRequest {
    pub(super) direction: Option<String>,
    pub(super) local_path: Option<String>,
    pub(super) remote_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SftpMkdirRequest {
    pub(super) parent_path: Option<String>,
    pub(super) name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SftpRenameRequest {
    pub(super) from_path: Option<String>,
    pub(super) to_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SftpDeleteRequest {
    pub(super) path: Option<String>,
    pub(super) recursive: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoteEntry {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) is_dir: bool,
    pub(super) size: Option<u64>,
    pub(super) modified_at: Option<String>,
}
