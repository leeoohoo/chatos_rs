use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct ProjectManagementSkillResponse {
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectServiceProjectRecord {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreateProjectServiceProjectRequest {
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateProjectServiceProjectRequest {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

pub async fn list_project_service_projects(
    base_url: &str,
    access_token: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let endpoint = format!("{}/api/projects", base_url.trim().trim_end_matches('/'));
    let mut request = reqwest::Client::new()
        .get(endpoint)
        .bearer_auth(access_token.trim());
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.query(&[("status", status)]);
    }
    send_json(request).await
}

pub async fn get_project_service_project(
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        reqwest::Client::new()
            .get(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn create_project_service_project(
    base_url: &str,
    access_token: &str,
    request: &CreateProjectServiceProjectRequest,
) -> Result<ProjectServiceProjectRecord, String> {
    let endpoint = format!("{}/api/projects", base_url.trim().trim_end_matches('/'));
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn update_project_service_project(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    request: &UpdateProjectServiceProjectRequest,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        reqwest::Client::new()
            .patch(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn archive_project_service_project(
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        reqwest::Client::new()
            .delete(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn fetch_project_management_skill(base_url: &str, lang: &str) -> Result<String, String> {
    let normalized_lang = match lang.trim() {
        "en" | "en-US" | "english" => "en-US",
        _ => "zh-CN",
    };
    let endpoint = format!(
        "{}/api/skills/project-management?lang={}",
        base_url.trim().trim_end_matches('/'),
        normalized_lang
    );
    let payload: ProjectManagementSkillResponse =
        send_json(reqwest::Client::new().get(endpoint)).await?;
    Ok(payload.content)
}

pub async fn sync_list_project_service_projects(
    base_url: &str,
    sync_secret: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/chatos-sync/projects",
        base_url.trim().trim_end_matches('/')
    );
    let mut request = reqwest::Client::new()
        .get(endpoint)
        .header("X-Project-Service-Sync-Secret", sync_secret.trim());
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.query(&[("status", status)]);
    }
    send_json(request).await
}

pub async fn sync_get_project_service_project(
    base_url: &str,
    sync_secret: &str,
    project_id: &str,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/chatos-sync/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        reqwest::Client::new()
            .get(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim()),
    )
    .await
}

async fn send_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

async fn send_optional_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<Option<T>, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    response
        .json::<T>()
        .await
        .map(Some)
        .map_err(|err| err.to_string())
}
