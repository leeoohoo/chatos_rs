use super::*;

impl RemoteServerService {
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
