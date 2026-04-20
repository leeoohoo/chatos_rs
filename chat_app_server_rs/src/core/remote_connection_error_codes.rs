use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub mod remote_connection_codes {
    pub const INVALID_ARGUMENT: &str = "invalid_argument";
    pub const USER_SCOPE_FORBIDDEN: &str = "user_scope_forbidden";
    pub const REMOTE_CONNECTION_NOT_FOUND: &str = "remote_connection_not_found";
    pub const REMOTE_CONNECTION_FORBIDDEN: &str = "remote_connection_forbidden";
    pub const REMOTE_CONNECTION_ACCESS_INTERNAL: &str = "remote_connection_access_internal";
    pub const REMOTE_CONNECTION_CREATE_FAILED: &str = "remote_connection_create_failed";
    pub const REMOTE_CONNECTION_UPDATE_FAILED: &str = "remote_connection_update_failed";
    pub const REMOTE_CONNECTION_FETCH_FAILED: &str = "remote_connection_fetch_failed";
    pub const REMOTE_CONNECTION_DELETE_FAILED: &str = "remote_connection_delete_failed";
    pub const HOST_KEY_MISMATCH: &str = "host_key_mismatch";
    pub const HOST_KEY_UNTRUSTED: &str = "host_key_untrusted";
    pub const HOST_KEY_VERIFICATION_FAILED: &str = "host_key_verification_failed";
    pub const AUTH_FAILED: &str = "auth_failed";
    pub const SECOND_FACTOR_REQUIRED: &str = "second_factor_required";
    pub const DNS_RESOLVE_FAILED: &str = "dns_resolve_failed";
    pub const NETWORK_TIMEOUT: &str = "network_timeout";
    pub const NETWORK_UNREACHABLE: &str = "network_unreachable";
    pub const CONNECTIVITY_TEST_FAILED: &str = "connectivity_test_failed";
    pub const TERMINAL_INIT_FAILED: &str = "terminal_init_failed";
    pub const TERMINAL_INPUT_FAILED: &str = "terminal_input_failed";
    pub const TERMINAL_RESIZE_FAILED: &str = "terminal_resize_failed";
    pub const INVALID_WS_MESSAGE: &str = "invalid_ws_message";
    pub const REMOTE_TERMINAL_ERROR: &str = "remote_terminal_error";

    pub const ALL: &[&str] = &[
        INVALID_ARGUMENT,
        USER_SCOPE_FORBIDDEN,
        REMOTE_CONNECTION_NOT_FOUND,
        REMOTE_CONNECTION_FORBIDDEN,
        REMOTE_CONNECTION_ACCESS_INTERNAL,
        REMOTE_CONNECTION_CREATE_FAILED,
        REMOTE_CONNECTION_UPDATE_FAILED,
        REMOTE_CONNECTION_FETCH_FAILED,
        REMOTE_CONNECTION_DELETE_FAILED,
        HOST_KEY_MISMATCH,
        HOST_KEY_UNTRUSTED,
        HOST_KEY_VERIFICATION_FAILED,
        AUTH_FAILED,
        SECOND_FACTOR_REQUIRED,
        DNS_RESOLVE_FAILED,
        NETWORK_TIMEOUT,
        NETWORK_UNREACHABLE,
        CONNECTIVITY_TEST_FAILED,
        TERMINAL_INIT_FAILED,
        TERMINAL_INPUT_FAILED,
        TERMINAL_RESIZE_FAILED,
        INVALID_WS_MESSAGE,
        REMOTE_TERMINAL_ERROR,
    ];
}

pub mod remote_sftp_codes {
    pub const BAD_REQUEST: &str = "bad_request";
    pub const INVALID_ARGUMENT: &str = "invalid_argument";
    pub const INVALID_PATH: &str = "invalid_path";
    pub const INVALID_DIRECTORY_NAME: &str = "invalid_directory_name";
    pub const SECOND_FACTOR_REQUIRED: &str = "second_factor_required";
    pub const TRANSFER_NOT_FOUND: &str = "transfer_not_found";
    pub const TRANSFER_NOT_ACTIVE: &str = "transfer_not_active";
    pub const TRANSFER_CANCELLED: &str = "transfer_cancelled";
    pub const TIMEOUT: &str = "timeout";
    pub const LOCAL_IO_ERROR: &str = "local_io_error";
    pub const REMOTE_AUTH_FAILED: &str = "remote_auth_failed";
    pub const REMOTE_PATH_NOT_FOUND: &str = "remote_path_not_found";
    pub const REMOTE_PERMISSION_DENIED: &str = "remote_permission_denied";
    pub const REMOTE_NETWORK_DISCONNECTED: &str = "remote_network_disconnected";
    pub const REMOTE_ERROR: &str = "remote_error";

    pub const ALL: &[&str] = &[
        BAD_REQUEST,
        INVALID_ARGUMENT,
        INVALID_PATH,
        INVALID_DIRECTORY_NAME,
        SECOND_FACTOR_REQUIRED,
        TRANSFER_NOT_FOUND,
        TRANSFER_NOT_ACTIVE,
        TRANSFER_CANCELLED,
        TIMEOUT,
        LOCAL_IO_ERROR,
        REMOTE_AUTH_FAILED,
        REMOTE_PATH_NOT_FOUND,
        REMOTE_PERMISSION_DENIED,
        REMOTE_NETWORK_DISCONNECTED,
        REMOTE_ERROR,
    ];
}

#[derive(Debug, Serialize)]
struct RemoteConnectionErrorCodeCatalog<'a> {
    remote_connection_codes: &'a [&'a str],
    remote_sftp_codes: &'a [&'a str],
}

pub fn remote_connection_error_code_catalog_json() -> Result<String, String> {
    let payload = RemoteConnectionErrorCodeCatalog {
        remote_connection_codes: remote_connection_codes::ALL,
        remote_sftp_codes: remote_sftp_codes::ALL,
    };
    let mut json = serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?;
    json.push('\n');
    Ok(json)
}

pub fn default_remote_connection_error_code_doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("remote_connection_error_codes.json")
}

pub fn export_remote_connection_error_code_catalog_to(path: &Path) -> Result<(), String> {
    let rendered = remote_connection_error_code_catalog_json()?;
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == rendered {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, rendered).map_err(|err| err.to_string())
}

pub fn export_remote_connection_error_code_catalog_doc() -> Result<PathBuf, String> {
    let path = default_remote_connection_error_code_doc_path();
    export_remote_connection_error_code_catalog_to(path.as_path())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{
        default_remote_connection_error_code_doc_path,
        export_remote_connection_error_code_catalog_doc, remote_connection_codes,
        remote_connection_error_code_catalog_json, remote_sftp_codes,
    };

    #[test]
    fn exports_default_doc_file_with_latest_catalog() {
        let path =
            export_remote_connection_error_code_catalog_doc().expect("export should succeed");
        assert_eq!(path, default_remote_connection_error_code_doc_path());
        let actual = std::fs::read_to_string(path).expect("doc should be readable");
        let expected = remote_connection_error_code_catalog_json().expect("render should succeed");
        assert_eq!(actual, expected);
    }

    #[test]
    fn keeps_codes_unique_inside_each_catalog() {
        let connection_unique = remote_connection_codes::ALL
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(connection_unique, remote_connection_codes::ALL.len());

        let sftp_unique = remote_sftp_codes::ALL
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(sftp_unique, remote_sftp_codes::ALL.len());
    }
}
