use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct UserScopeQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ModelConfigGetQuery {
    pub(super) include_secret: Option<bool>,
}
