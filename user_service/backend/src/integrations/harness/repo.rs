// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};

use crate::models::HARNESS_PROVISIONING_STATUS_PROVISIONED;
use crate::secrets::decrypt_secret;
use crate::state::AppState;

use super::super::http::normalized_url;
use super::harness_request_json;
use super::identifiers::harness_repo_identifier;
#[derive(Debug, Serialize)]
struct HarnessCreateRepoRequest<'a> {
    parent_ref: &'a str,
    identifier: &'a str,
    default_branch: &'a str,
    description: &'a str,
    is_public: bool,
    readme: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessProjectRepoCreateRequest {
    pub project_id: String,
    pub project_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessProjectRepoResponse {
    pub space_identifier: String,
    pub repo_identifier: String,
    pub repo_path: String,
    pub git_url: String,
    pub git_ssh_url: Option<String>,
    pub default_branch: String,
    pub push_username: String,
    pub push_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessApiAccessResponse {
    pub base_url: String,
    pub access_token: String,
    pub harness_uid: String,
    pub space_identifier: String,
}

#[derive(Debug, Deserialize)]
struct HarnessRepositoryOutput {
    identifier: String,
    path: String,
    #[serde(default)]
    git_url: String,
    #[serde(default)]
    git_ssh_url: Option<String>,
    #[serde(default)]
    default_branch: Option<String>,
}

pub async fn create_harness_project_repo(
    state: &AppState,
    owner_user_id: &str,
    input: HarnessProjectRepoCreateRequest,
) -> Result<HarnessProjectRepoResponse, String> {
    if !state.config.harness_provisioning_enabled {
        return Err("harness provisioning is disabled".to_string());
    }
    let base_url = normalized_url(state.config.harness_base_url.as_deref())
        .ok_or_else(|| "HARNESS_BASE_URL is not configured".to_string())?;
    let record = state
        .store
        .find_harness_provisioning_by_user_id(owner_user_id)
        .await?
        .ok_or_else(|| "harness provisioning record not found".to_string())?;
    if record.status != HARNESS_PROVISIONING_STATUS_PROVISIONED {
        return Err(format!(
            "harness provisioning is not ready: {}",
            record.status
        ));
    }
    let encrypted_access_token = record
        .encrypted_access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "harness access token is unavailable; login again or retry provisioning".to_string()
        })?;
    let push_token = decrypt_secret(encrypted_access_token)?;
    let repo_identifier =
        harness_repo_identifier(input.project_name.as_str(), input.project_id.as_str());
    let description = input
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Chatos project");
    let body = HarnessCreateRepoRequest {
        parent_ref: record.space_identifier.as_str(),
        identifier: repo_identifier.as_str(),
        default_branch: "main",
        description,
        is_public: false,
        readme: false,
    };
    let endpoint = format!("{base_url}/api/v1/repos");
    let repo = match harness_request_json::<HarnessRepositoryOutput, _>(
        state,
        Method::POST,
        endpoint.as_str(),
        Some(push_token.as_str()),
        Some(&body),
    )
    .await
    {
        Ok(repo) => repo,
        Err(err) if err.is_already_exists() => {
            let endpoint = format!(
                "{base_url}/api/v1/repos/{}/{}",
                urlencoding::encode(record.space_identifier.as_str()),
                urlencoding::encode(repo_identifier.as_str())
            );
            harness_request_json::<HarnessRepositoryOutput, ()>(
                state,
                Method::GET,
                endpoint.as_str(),
                Some(push_token.as_str()),
                None,
            )
            .await
            .map_err(|fetch_err| {
                format!(
                    "create harness repo reported existing, but read existing repo failed: {fetch_err}"
                )
            })?
        }
        Err(err) => return Err(format!("create harness repo failed: {err}")),
    };
    let expected_repo_path = format!("{}/{}", record.space_identifier, repo_identifier);
    if repo.identifier.trim() != repo_identifier || repo.path.trim() != expected_repo_path {
        return Err(format!(
            "existing Harness repo does not match requested user space: expected {expected_repo_path}, got {}",
            repo.path
        ));
    }
    let git_url = rewrite_harness_local_url_host(repo.git_url.as_str(), &base_url, true);
    if git_url.is_empty() {
        return Err("harness repo response missing git_url".to_string());
    }
    Ok(HarnessProjectRepoResponse {
        space_identifier: record.space_identifier,
        repo_identifier: repo.identifier,
        repo_path: repo.path,
        git_url,
        git_ssh_url: repo
            .git_ssh_url
            .map(|value| rewrite_harness_local_url_host(value.as_str(), &base_url, false))
            .filter(|value| !value.is_empty()),
        default_branch: repo
            .default_branch
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "main".to_string()),
        push_username: record.harness_uid,
        push_token,
    })
}

pub async fn get_harness_api_access_for_user(
    state: &AppState,
    owner_user_id: &str,
) -> Result<HarnessApiAccessResponse, String> {
    if !state.config.harness_provisioning_enabled {
        return Err("harness provisioning is disabled".to_string());
    }
    let base_url = normalized_url(state.config.harness_base_url.as_deref())
        .ok_or_else(|| "HARNESS_BASE_URL is not configured".to_string())?;
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err("owner_user_id is required".to_string());
    }
    let record = state
        .store
        .find_harness_provisioning_by_user_id(owner_user_id)
        .await?
        .ok_or_else(|| "harness provisioning record not found".to_string())?;
    if record.status != HARNESS_PROVISIONING_STATUS_PROVISIONED {
        return Err(format!(
            "harness provisioning is not ready: {}",
            record.status
        ));
    }
    let encrypted_access_token = record
        .encrypted_access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "harness access token is unavailable; login again or retry provisioning".to_string()
        })?;
    Ok(HarnessApiAccessResponse {
        base_url,
        access_token: decrypt_secret(encrypted_access_token)?,
        harness_uid: record.harness_uid,
        space_identifier: record.space_identifier,
    })
}

fn rewrite_harness_local_url_host(
    raw_url: &str,
    harness_base_url: &str,
    rewrite_origin: bool,
) -> String {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Ok(mut url) = Url::parse(trimmed) else {
        return trimmed.to_string();
    };
    let Some(current_host) = url.host_str() else {
        return trimmed.to_string();
    };
    if !is_local_harness_host(current_host) {
        return trimmed.to_string();
    }

    let Ok(base_url) = Url::parse(harness_base_url) else {
        return trimmed.to_string();
    };
    let Some(base_host) = base_url.host_str() else {
        return trimmed.to_string();
    };

    if rewrite_origin {
        let _ = url.set_scheme(base_url.scheme());
        let _ = url.set_port(base_url.port());
    }
    let _ = url.set_host(Some(base_host));
    url.to_string()
}

fn is_local_harness_host(host: &str) -> bool {
    let normalized = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    normalized == "localhost"
        || normalized == "::1"
        || normalized == "0.0.0.0"
        || normalized.starts_with("127.")
}

#[cfg(test)]
mod tests {
    use super::rewrite_harness_local_url_host;

    #[test]
    fn harness_repo_git_url_rewrites_localhost_to_configured_base_url() {
        assert_eq!(
            rewrite_harness_local_url_host(
                "http://localhost:3000/git/u-leeoohoo/project.git",
                "http://8.155.171.124:3000",
                true,
            ),
            "http://8.155.171.124:3000/git/u-leeoohoo/project.git"
        );
        assert_eq!(
            rewrite_harness_local_url_host(
                "ssh://git@localhost:3022/u-leeoohoo/project.git",
                "http://8.155.171.124:3000",
                false,
            ),
            "ssh://git@8.155.171.124:3022/u-leeoohoo/project.git"
        );
    }

    #[test]
    fn harness_repo_git_url_keeps_non_local_hosts() {
        assert_eq!(
            rewrite_harness_local_url_host(
                "https://git.example.com/u-leeoohoo/project.git",
                "http://8.155.171.124:3000",
                true,
            ),
            "https://git.example.com/u-leeoohoo/project.git"
        );
    }
}
