use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ProjectQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateProjectRequest {
    pub(super) name: Option<String>,
    pub(super) root_path: Option<String>,
    pub(super) description: Option<String>,
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateProjectRequest {
    pub(super) name: Option<String>,
    pub(super) root_path: Option<String>,
    pub(super) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectContactsQuery {
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AddProjectContactRequest {
    pub(super) contact_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectChangeQuery {
    pub(super) path: Option<String>,
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ConfirmProjectChangesRequest {
    pub(super) mode: Option<String>,
    pub(super) paths: Option<Vec<String>>,
    pub(super) change_ids: Option<Vec<String>>,
}
