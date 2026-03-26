use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct FsQuery {
    pub(super) path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsReadQuery {
    pub(super) path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsSearchQuery {
    pub(super) path: Option<String>,
    pub(super) q: Option<String>,
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsMkdirRequest {
    pub(super) parent_path: Option<String>,
    pub(super) name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsCreateFileRequest {
    pub(super) parent_path: Option<String>,
    pub(super) name: Option<String>,
    pub(super) content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsDeleteRequest {
    pub(super) path: Option<String>,
    pub(super) recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsMoveRequest {
    pub(super) source_path: Option<String>,
    pub(super) target_parent_path: Option<String>,
    pub(super) target_name: Option<String>,
    pub(super) replace_existing: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsDownloadQuery {
    pub(super) path: Option<String>,
}
