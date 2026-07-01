// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
}
