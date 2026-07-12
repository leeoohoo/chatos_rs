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
        if let Some(existing) = find_reusable_remote_server(&self.store, &record).await? {
            return Ok(existing);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, StoreMode};
    use crate::models::UserRole;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://remote-server-service-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir: ".".to_string(),
            memory_timeout: Duration::from_millis(1000),
            execution_timeout: Duration::from_millis(1000),
            scheduler_poll_interval: Duration::from_millis(1000),
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1000,
            default_tool_results_model_total_max_chars: 2000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
            chatos_internal_api_secret: None,
            local_connector_internal_api_secret: None,
            callback_timeout: Duration::from_millis(1000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5000),
        }
    }

    async fn test_service() -> RemoteServerService {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        RemoteServerService::new(store)
    }

    fn agent_user(owner_user_id: &str) -> CurrentUser {
        CurrentUser {
            id: format!("agent-{owner_user_id}"),
            username: format!("agent-{owner_user_id}"),
            display_name: format!("Agent {owner_user_id}"),
            role: UserRole::Agent,
            owner_user_id: Some(owner_user_id.to_string()),
            owner_username: Some(format!("user-{owner_user_id}")),
            owner_display_name: Some(format!("User {owner_user_id}")),
        }
    }

    fn remote_server_request(name: &str) -> CreateRemoteServerRequest {
        CreateRemoteServerRequest {
            name: name.to_string(),
            host: "8.155.171.124".to_string(),
            port: Some(22),
            username: "root".to_string(),
            auth_type: "password".to_string(),
            password: Some("secret".to_string()),
            private_key_path: None,
            certificate_path: None,
            default_remote_path: None,
            host_key_policy: Some("accept_new".to_string()),
            enabled: Some(true),
        }
    }

    #[tokio::test]
    async fn create_remote_server_reuses_matching_existing_server() {
        let service = test_service().await;
        let creator = agent_user("owner-a");

        let first = service
            .create_remote_server(remote_server_request("first name"), Some(&creator))
            .await
            .expect("create first server");
        let second = service
            .create_remote_server(remote_server_request("second name"), Some(&creator))
            .await
            .expect("reuse matching server");

        assert_eq!(second.id, first.id);
        assert_eq!(second.name, first.name);
        let servers = service
            .store
            .list_remote_servers()
            .await
            .expect("list remote servers");
        assert_eq!(servers.len(), 1);
    }
}
