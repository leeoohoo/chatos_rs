// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PluginOAuthBroker {
    pub(super) async fn refresh_connection_if_needed(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
    ) -> Result<LocalPluginOAuthConnection> {
        let storage_key = connection_key_for(expected);
        let refresh_lock = self.refresh_lock(storage_key.as_str())?;
        let _refresh_guard = refresh_lock.lock().await;
        let current = self.load_exact_bound_connection(expected, expected_binding_sha256)?;
        if !connection_needs_refresh(&current)? {
            validate_connection_expiry(&current)?;
            return Ok(current);
        }

        let app = load_oauth_app(
            &self.installer,
            current.plugin_id.as_str(),
            current.release_id.as_str(),
            current.component_key.as_str(),
        )?;
        if app.provider != current.provider || app.resource != current.resource {
            bail!("Plugin OAuth Connected App changed after authorization");
        }
        let refresh_token = match self.resolve_refresh_token(&current) {
            Ok(refresh_token) => refresh_token,
            Err(error) => {
                if validate_connection_expiry(&current).is_ok() {
                    return Ok(current);
                }
                let _ = self.mark_connection_needs_auth(expected, expected_binding_sha256);
                return Err(error)
                    .context("Plugin OAuth access token expired without a refresh token");
            }
        };
        let token = match self
            .exchange_refresh_token(&app, refresh_token.as_str())
            .await
        {
            Ok(token) => token,
            Err(error) => {
                let _ = self.mark_connection_needs_auth(expected, expected_binding_sha256);
                return Err(error).context("refresh Plugin OAuth connection");
            }
        };
        drop(refresh_token);
        match self.persist_refreshed_connection(expected, expected_binding_sha256, &app, token) {
            Ok(connection) => Ok(connection),
            Err(error) => {
                let _ = self.mark_connection_needs_auth(expected, expected_binding_sha256);
                Err(error).context("persist refreshed Plugin OAuth connection")
            }
        }
    }

    fn refresh_lock(&self, storage_key: &str) -> Result<Arc<tokio::sync::Mutex<()>>> {
        let mut locks = self
            .refresh_locks
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin OAuth refresh lock registry is poisoned"))?;
        Ok(locks
            .entry(storage_key.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone())
    }

    fn load_exact_bound_connection(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
    ) -> Result<LocalPluginOAuthConnection> {
        let _guard = self.operation_guard()?;
        let state = load_state(self.state_path.as_path())?;
        let connection = state
            .connections
            .get(connection_key_for(expected).as_str())
            .context("Plugin OAuth connection no longer exists")?;
        self.verify_connection_seal(connection)?;
        if !connection.connected || connection.needs_auth {
            bail!("Plugin OAuth connection requires authorization");
        }
        if connection.id != expected.id
            || connection_binding_sha256(connection) != expected_binding_sha256
        {
            bail!("Plugin OAuth connection changed after prepare");
        }
        Ok(connection.clone())
    }

    fn resolve_refresh_token(
        &self,
        connection: &LocalPluginOAuthConnection,
    ) -> Result<ResolvedOAuthRefreshToken> {
        let scope = token_scope(connection, REFRESH_TOKEN_SECRET)?;
        let handle = self.vault.issue_handle(&scope, TOKEN_HANDLE_TTL)?;
        let resolved = self.vault.resolve_handle(handle.as_str(), &scope);
        let _ = self.vault.revoke_handle(handle.as_str());
        let secret = resolved?;
        let value = std::str::from_utf8(secret.as_bytes())
            .context("Plugin OAuth refresh token is not UTF-8")?
            .to_string();
        Ok(ResolvedOAuthRefreshToken(value))
    }

    fn persist_refreshed_connection(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
        app: &PluginOAuthAppManifest,
        mut token: OAuthTokenResponse,
    ) -> Result<LocalPluginOAuthConnection> {
        let _guard = self.operation_guard()?;
        let mut state = load_state(self.state_path.as_path())?;
        let storage_key = connection_key_for(expected);
        let current = state
            .connections
            .get(storage_key.as_str())
            .context("Plugin OAuth connection no longer exists")?;
        self.verify_connection_seal(current)?;
        if !current.connected
            || current.needs_auth
            || current.id != expected.id
            || connection_binding_sha256(current) != expected_binding_sha256
        {
            zeroize_token_response(&mut token);
            bail!("Plugin OAuth connection changed while refreshing");
        }
        let scopes = match refreshed_scopes(current, app, token.scope.as_deref()) {
            Ok(scopes) => scopes,
            Err(error) => {
                zeroize_token_response(&mut token);
                return Err(error);
            }
        };
        let Some(expires_at) = token_expiry(token.expires_in) else {
            zeroize_token_response(&mut token);
            bail!("Plugin OAuth refresh response omitted expires_in");
        };
        let mut connection = current.clone();
        connection.scopes = scopes;
        connection.connected = true;
        connection.needs_auth = false;
        connection.expires_at = Some(expires_at);
        connection.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

        let access_scope = token_scope(&connection, ACCESS_TOKEN_SECRET)?;
        if let Err(error) = self
            .vault
            .upsert(&access_scope, token.access_token.as_bytes())
        {
            zeroize_token_response(&mut token);
            return Err(error);
        }
        token.access_token.zeroize();
        if let Some(mut refresh_token) = token.refresh_token.take() {
            let refresh_scope = token_scope(&connection, REFRESH_TOKEN_SECRET)?;
            if let Err(error) = self.vault.upsert(&refresh_scope, refresh_token.as_bytes()) {
                refresh_token.zeroize();
                return Err(error);
            }
            refresh_token.zeroize();
        }
        let seal_scope = token_scope(&connection, CONNECTION_SEAL_SECRET)?;
        self.vault.upsert(
            &seal_scope,
            connection_snapshot_sha256(&connection).as_bytes(),
        )?;
        state.connections.insert(storage_key, connection.clone());
        save_state(self.state_path.as_path(), &state)?;
        Ok(connection)
    }

    fn mark_connection_needs_auth(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
    ) -> Result<()> {
        let _guard = self.operation_guard()?;
        let mut state = load_state(self.state_path.as_path())?;
        let storage_key = connection_key_for(expected);
        let Some(current) = state.connections.get(storage_key.as_str()) else {
            return Ok(());
        };
        self.verify_connection_seal(current)?;
        if current.id != expected.id
            || connection_binding_sha256(current) != expected_binding_sha256
        {
            return Ok(());
        }
        let mut connection = current.clone();
        connection.connected = false;
        connection.needs_auth = true;
        connection.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let _ = self
            .vault
            .delete(&token_scope(&connection, ACCESS_TOKEN_SECRET)?)?;
        let _ = self
            .vault
            .delete(&token_scope(&connection, REFRESH_TOKEN_SECRET)?)?;
        self.vault.upsert(
            &token_scope(&connection, CONNECTION_SEAL_SECRET)?,
            connection_snapshot_sha256(&connection).as_bytes(),
        )?;
        state.connections.insert(storage_key, connection);
        save_state(self.state_path.as_path(), &state)
    }

    pub(super) fn verify_connection_seal(
        &self,
        connection: &LocalPluginOAuthConnection,
    ) -> Result<()> {
        let scope = token_scope(connection, CONNECTION_SEAL_SECRET)?;
        let handle = self.vault.issue_handle(&scope, TOKEN_HANDLE_TTL)?;
        let resolved = self.vault.resolve_handle(handle.as_str(), &scope);
        let _ = self.vault.revoke_handle(handle.as_str());
        let seal = resolved?;
        if seal.as_bytes() != connection_snapshot_sha256(connection).as_bytes() {
            bail!("Plugin OAuth connection metadata seal mismatch");
        }
        Ok(())
    }
}
