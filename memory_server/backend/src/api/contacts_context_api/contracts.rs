use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(in crate::api) struct ListContactMemoriesQuery {
    pub(super) project_id: Option<String>,
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct ListContactProjectsQuery {
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
}
