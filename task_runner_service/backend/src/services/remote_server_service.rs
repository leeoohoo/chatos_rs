use crate::auth::CurrentUser;
use crate::models::{
    now_rfc3339, CreateRemoteServerRequest, RemoteServerRecord, RemoteServerTestResponse,
    TestRemoteServerRequest, UpdateRemoteServerRequest,
};
use crate::remote_server_runtime::test_remote_server_connectivity;
use crate::store::AppStore;

use super::remote_servers::{
    build_remote_server_record, normalize_remote_server_auth_type,
    normalize_remote_server_host_key_policy, normalize_remote_server_port,
    validate_remote_server_auth_fields,
};
use super::{normalized_optional, validate_required, RemoteServerService};

impl RemoteServerService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }

    async fn first_task_referencing_server(
        &self,
        server_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .find(|task| task.mcp_config.default_remote_server_id.as_deref() == Some(server_id))
            .map(|task| task.id))
    }

    pub async fn list_remote_servers(&self) -> Result<Vec<RemoteServerRecord>, String> {
        self.store.list_remote_servers().await
    }

    pub async fn get_remote_server(&self, id: &str) -> Result<Option<RemoteServerRecord>, String> {
        self.store.get_remote_server(id).await
    }

    pub async fn create_remote_server(
        &self,
        input: CreateRemoteServerRequest,
        creator: Option<&CurrentUser>,
    ) -> Result<RemoteServerRecord, String> {
        let now = now_rfc3339();
        let record = build_remote_server_record(input, creator, None, now)?;
        self.store.save_remote_server(record).await
    }

    pub async fn update_remote_server(
        &self,
        id: &str,
        patch: UpdateRemoteServerRequest,
    ) -> Result<Option<RemoteServerRecord>, String> {
        let Some(mut record) = self.store.get_remote_server(id).await? else {
            return Ok(None);
        };

        if let Some(name) = patch.name {
            validate_required("name", &name)?;
            record.name = name.trim().to_string();
        }
        if let Some(host) = patch.host {
            validate_required("host", &host)?;
            record.host = host.trim().to_string();
        }
        if let Some(port) = patch.port {
            record.port = normalize_remote_server_port(Some(port))?;
        }
        if let Some(username) = patch.username {
            validate_required("username", &username)?;
            record.username = username.trim().to_string();
        }
        if let Some(auth_type) = patch.auth_type {
            validate_required("auth_type", &auth_type)?;
            record.auth_type = normalize_remote_server_auth_type(&auth_type)?;
        }
        if let Some(password) = patch.password {
            record.password = normalized_optional(Some(password));
        }
        if let Some(private_key_path) = patch.private_key_path {
            record.private_key_path = normalized_optional(Some(private_key_path));
        }
        if let Some(certificate_path) = patch.certificate_path {
            record.certificate_path = normalized_optional(Some(certificate_path));
        }
        if let Some(default_remote_path) = patch.default_remote_path {
            record.default_remote_path = normalized_optional(Some(default_remote_path));
        }
        if let Some(host_key_policy) = patch.host_key_policy {
            record.host_key_policy =
                normalize_remote_server_host_key_policy(Some(host_key_policy.as_str()))?;
        }
        if let Some(enabled) = patch.enabled {
            if !enabled {
                if let Some(task_id) = self.first_task_referencing_server(id).await? {
                    return Err(format!("远程服务器仍被任务引用，暂时不能停用: {task_id}"));
                }
            }
            record.enabled = enabled;
        }
        validate_remote_server_auth_fields(&record)?;
        record.updated_at = now_rfc3339();
        Ok(Some(self.store.save_remote_server(record).await?))
    }

    pub async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        if let Some(task_id) = self.first_task_referencing_server(id).await? {
            return Err(format!("远程服务器仍被任务引用，暂时不能删除: {task_id}"));
        }
        self.store.delete_remote_server(id).await
    }

    pub async fn test_remote_server_draft(
        &self,
        input: TestRemoteServerRequest,
    ) -> Result<RemoteServerTestResponse, String> {
        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("draft");
        let host = input
            .host
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "host is required".to_string())?;
        let username = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "username is required".to_string())?;
        let auth_type = input
            .auth_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "auth_type is required".to_string())?;
        let now = now_rfc3339();
        let draft = RemoteServerRecord {
            id: "draft".to_string(),
            name: name.to_string(),
            host: host.to_string(),
            port: normalize_remote_server_port(input.port)?,
            username: username.to_string(),
            auth_type: normalize_remote_server_auth_type(auth_type)?,
            password: normalized_optional(input.password),
            private_key_path: normalized_optional(input.private_key_path),
            certificate_path: normalized_optional(input.certificate_path),
            default_remote_path: normalized_optional(input.default_remote_path),
            host_key_policy: normalize_remote_server_host_key_policy(
                input.host_key_policy.as_deref(),
            )?,
            enabled: true,
            last_tested_at: None,
            last_test_status: None,
            last_test_message: None,
            last_active_at: None,
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            task_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        validate_remote_server_auth_fields(&draft)?;

        Ok(match test_remote_server_connectivity(&draft, None).await {
            Ok(response) => response,
            Err(err) => RemoteServerTestResponse {
                ok: false,
                server_id: None,
                name: draft.name,
                host: draft.host,
                port: draft.port,
                username: draft.username,
                auth_type: draft.auth_type,
                remote_host: None,
                error: Some(err),
                tested_at: now_rfc3339(),
            },
        })
    }

    pub async fn test_remote_server_saved(
        &self,
        id: &str,
    ) -> Result<Option<RemoteServerTestResponse>, String> {
        let Some(mut record) = self.store.get_remote_server(id).await? else {
            return Ok(None);
        };

        let response = match test_remote_server_connectivity(&record, Some(record.id.clone())).await
        {
            Ok(response) => {
                record.last_tested_at = Some(response.tested_at.clone());
                record.last_test_status = Some("success".to_string());
                record.last_test_message = response.remote_host.clone();
                record.updated_at = now_rfc3339();
                self.store.save_remote_server(record).await?;
                response
            }
            Err(err) => {
                let tested_at = now_rfc3339();
                record.last_tested_at = Some(tested_at.clone());
                record.last_test_status = Some("failed".to_string());
                record.last_test_message = Some(err.clone());
                record.updated_at = now_rfc3339();
                self.store.save_remote_server(record.clone()).await?;
                RemoteServerTestResponse {
                    ok: false,
                    server_id: Some(record.id),
                    name: record.name,
                    host: record.host,
                    port: record.port,
                    username: record.username,
                    auth_type: record.auth_type,
                    remote_host: None,
                    error: Some(err),
                    tested_at,
                }
            }
        };

        Ok(Some(response))
    }
}
