use crate::{
    domain::{datasource::DataSource, meta::AuthMode},
    error::{AppError, AppResult},
};

pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5_000;
pub const DEFAULT_POOL_MIN: u32 = 1;
pub const DEFAULT_POOL_MAX: u32 = 20;
pub const DEFAULT_SQLITE_POOL_MAX: u32 = 5;

pub fn connect_timeout_ms(datasource: &DataSource) -> u64 {
    datasource
        .options
        .connect_timeout_ms
        .unwrap_or(DEFAULT_CONNECT_TIMEOUT_MS)
}

pub fn pool_limits(datasource: &DataSource, default_max: u32) -> (u32, u32) {
    (
        datasource.options.pool_min.unwrap_or(DEFAULT_POOL_MIN),
        datasource.options.pool_max.unwrap_or(default_max),
    )
}

pub fn validate_network_host_port(datasource: &DataSource) -> AppResult<()> {
    require_network_host(datasource)?;
    require_network_port(datasource)?;
    Ok(())
}

pub fn require_network_host(datasource: &DataSource) -> AppResult<String> {
    require_non_empty_string("network.host", datasource.network.host.as_deref())
}

pub fn require_network_port(datasource: &DataSource) -> AppResult<u16> {
    datasource
        .network
        .port
        .ok_or_else(|| AppError::BadRequest("network.port is required".to_string()))
}

pub fn require_sqlite_file_path(datasource: &DataSource) -> AppResult<String> {
    require_non_empty_string("network.file_path", datasource.network.file_path.as_deref())
        .map_err(|_| AppError::BadRequest("network.file_path is required for sqlite".to_string()))
}

pub fn validate_supported_auth_mode(
    datasource: &DataSource,
    supported: &[AuthMode],
    message: &str,
) -> AppResult<()> {
    if supported.contains(&datasource.auth.mode) {
        Ok(())
    } else {
        Err(AppError::BadRequest(message.to_string()))
    }
}

pub fn validate_password_auth(datasource: &DataSource) -> AppResult<()> {
    require_username(datasource)?;
    require_password(datasource)?;
    Ok(())
}

pub fn validate_token_auth(datasource: &DataSource) -> AppResult<()> {
    require_access_token(datasource)?;
    Ok(())
}

pub fn validate_tls_client_cert_auth(datasource: &DataSource) -> AppResult<()> {
    require_client_cert(datasource)?;
    require_client_key(datasource)?;
    Ok(())
}

pub fn validate_file_key_reference(datasource: &DataSource) -> AppResult<()> {
    let has_ref = datasource
        .auth
        .key_ref
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || datasource
            .auth
            .wallet_ref
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    if has_ref {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "key_ref or wallet_ref is required".to_string(),
        ))
    }
}

pub fn require_username(datasource: &DataSource) -> AppResult<String> {
    require_non_empty_string("username", datasource.auth.username.as_deref())
}

pub fn require_password(datasource: &DataSource) -> AppResult<String> {
    require_non_empty_string("password", datasource.auth.password.as_deref())
}

pub fn require_access_token(datasource: &DataSource) -> AppResult<String> {
    require_non_empty_string("access_token", datasource.auth.access_token.as_deref())
}

pub fn require_client_cert(datasource: &DataSource) -> AppResult<String> {
    require_non_empty_string("client_cert", datasource.auth.client_cert.as_deref())
}

pub fn require_client_key(datasource: &DataSource) -> AppResult<String> {
    require_non_empty_string("client_key", datasource.auth.client_key.as_deref())
}

fn require_non_empty_string(field: &str, value: Option<&str>) -> AppResult<String> {
    let Some(value) = value else {
        return Err(AppError::BadRequest(format!("{field} is required")));
    };
    if value.trim().is_empty() {
        return Err(AppError::BadRequest(format!("{field} is required")));
    }
    Ok(value.to_string())
}
