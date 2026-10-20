use std::fmt::Display;

pub trait CredentialStore: Send + Sync {
    fn load_access_token(&self) -> Result<Option<String>, String>;
    fn save_access_token(&self, token: &str) -> Result<(), String>;
    fn delete_access_token(&self) -> Result<(), String>;
}

pub struct PlatformCredentialStore;

const SERVICE: &str = "com.chatos.windows.authentication";
const ACCOUNT: &str = "access-token";

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl CredentialStore for PlatformCredentialStore {
    fn load_access_token(&self) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(safe_credential_error)?;
        match entry.get_password() {
            Ok(token) => Ok(normalized_token(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(safe_credential_error(error)),
        }
    }

    fn save_access_token(&self, token: &str) -> Result<(), String> {
        let token = token.trim();
        if token.is_empty() {
            return self.delete_access_token();
        }
        keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(safe_credential_error)?
            .set_password(token)
            .map_err(safe_credential_error)
    }

    fn delete_access_token(&self) -> Result<(), String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(safe_credential_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(safe_credential_error(error)),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl CredentialStore for PlatformCredentialStore {
    fn load_access_token(&self) -> Result<Option<String>, String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn save_access_token(&self, _token: &str) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn delete_access_token(&self) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }
}

fn normalized_token(token: String) -> Option<String> {
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

fn safe_credential_error(error: impl Display) -> String {
    let _ = error;
    "无法访问系统凭据存储。".into()
}
