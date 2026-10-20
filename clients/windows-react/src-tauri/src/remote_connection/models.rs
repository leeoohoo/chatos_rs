use chatos_local_agent_protocol::{
    LocalRemoteAuthenticationType, LocalRemoteConnection, LocalRemoteHostKeyPolicy,
};
use serde::{Deserialize, Serialize};

const MAXIMUM_SECRET_BYTES: usize = 1_024;
const MAXIMUM_PATH_BYTES: usize = 1_024;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteConnectionInput {
    pub name: Option<String>,
    pub host: String,
    pub port: u32,
    pub username: String,
    pub authentication_type: LocalRemoteAuthenticationType,
    pub default_remote_path: Option<String>,
    pub host_key_policy: LocalRemoteHostKeyPolicy,
    pub jump_enabled: bool,
    pub jump_connection_id: Option<String>,
    pub jump_host: Option<String>,
    pub jump_port: Option<u32>,
    pub jump_username: Option<String>,
    #[serde(default)]
    pub credentials: RemoteCredentialMutation,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteCredentialMutation {
    #[serde(default)]
    pub password: RemoteSecretMutation,
    #[serde(default)]
    pub private_key_path: RemoteSecretMutation,
    #[serde(default)]
    pub certificate_path: RemoteSecretMutation,
    #[serde(default)]
    pub jump_password: RemoteSecretMutation,
    #[serde(default)]
    pub jump_private_key_path: RemoteSecretMutation,
    #[serde(default)]
    pub jump_certificate_path: RemoteSecretMutation,
}

#[derive(Clone, Default, Deserialize)]
#[serde(
    tag = "action",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RemoteSecretMutation {
    #[default]
    Keep,
    Clear,
    Replace(String),
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RemoteCredentialBundle {
    pub(super) password: Option<String>,
    pub(super) private_key_path: Option<String>,
    pub(super) certificate_path: Option<String>,
    pub(super) jump_password: Option<String>,
    pub(super) jump_private_key_path: Option<String>,
    pub(super) jump_certificate_path: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RemoteCredentialFlags {
    pub(super) has_password: bool,
    pub(super) has_private_key_path: bool,
    pub(super) has_certificate_path: bool,
    pub(super) has_jump_password: bool,
    pub(super) has_jump_private_key_path: bool,
    pub(super) has_jump_certificate_path: bool,
}

impl RemoteCredentialBundle {
    pub(super) fn applying(
        mut self,
        mutation: &RemoteCredentialMutation,
        authentication_type: LocalRemoteAuthenticationType,
        retain_manual_jump_credentials: bool,
    ) -> Result<Self, String> {
        apply_secret(
            &mut self.password,
            &mutation.password,
            "登录密码",
            MAXIMUM_SECRET_BYTES,
            false,
        )?;
        apply_secret(
            &mut self.private_key_path,
            &mutation.private_key_path,
            "私钥路径",
            MAXIMUM_PATH_BYTES,
            true,
        )?;
        apply_secret(
            &mut self.certificate_path,
            &mutation.certificate_path,
            "证书路径",
            MAXIMUM_PATH_BYTES,
            true,
        )?;
        apply_secret(
            &mut self.jump_password,
            &mutation.jump_password,
            "跳板机密码",
            MAXIMUM_SECRET_BYTES,
            false,
        )?;
        apply_secret(
            &mut self.jump_private_key_path,
            &mutation.jump_private_key_path,
            "跳板机私钥路径",
            MAXIMUM_PATH_BYTES,
            true,
        )?;
        apply_secret(
            &mut self.jump_certificate_path,
            &mutation.jump_certificate_path,
            "跳板机证书路径",
            MAXIMUM_PATH_BYTES,
            true,
        )?;

        match authentication_type {
            LocalRemoteAuthenticationType::Password => {
                self.private_key_path = None;
                self.certificate_path = None;
                if self.password.is_none() {
                    return Err("请输入登录密码。".into());
                }
            }
            LocalRemoteAuthenticationType::PrivateKey => {
                self.password = None;
                self.certificate_path = None;
                if self.private_key_path.is_none() {
                    return Err("请选择私钥文件。".into());
                }
            }
            LocalRemoteAuthenticationType::PrivateKeyCert => {
                self.password = None;
                if self.private_key_path.is_none() {
                    return Err("请选择私钥文件。".into());
                }
                if self.certificate_path.is_none() {
                    return Err("请选择 SSH 证书文件。".into());
                }
            }
        }

        if retain_manual_jump_credentials {
            if self.jump_certificate_path.is_some() && self.jump_private_key_path.is_none() {
                return Err("跳板机证书必须与跳板机私钥一起使用。".into());
            }
        } else {
            self.jump_password = None;
            self.jump_private_key_path = None;
            self.jump_certificate_path = None;
        }
        Ok(self)
    }

    pub(super) fn flags(&self) -> RemoteCredentialFlags {
        RemoteCredentialFlags {
            has_password: self.password.is_some(),
            has_private_key_path: self.private_key_path.is_some(),
            has_certificate_path: self.certificate_path.is_some(),
            has_jump_password: self.jump_password.is_some(),
            has_jump_private_key_path: self.jump_private_key_path.is_some(),
            has_jump_certificate_path: self.jump_certificate_path.is_some(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn apply_secret(
    target: &mut Option<String>,
    mutation: &RemoteSecretMutation,
    label: &str,
    maximum_bytes: usize,
    reject_control_characters: bool,
) -> Result<(), String> {
    match mutation {
        RemoteSecretMutation::Keep => {}
        RemoteSecretMutation::Clear => *target = None,
        RemoteSecretMutation::Replace(value) => {
            let value = value.trim();
            if value.is_empty()
                || value.len() > maximum_bytes
                || value.contains('\0')
                || (reject_control_characters && value.chars().any(char::is_control))
            {
                return Err(format!("{label}无效。"));
            }
            *target = Some(value.to_owned());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentedRemoteConnection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u32,
    pub username: String,
    pub authentication_type: LocalRemoteAuthenticationType,
    pub has_password: bool,
    pub has_private_key_path: bool,
    pub has_certificate_path: bool,
    pub default_remote_path: Option<String>,
    pub host_key_policy: LocalRemoteHostKeyPolicy,
    pub jump_enabled: bool,
    pub jump_connection_id: Option<String>,
    pub jump_host: Option<String>,
    pub jump_port: Option<u32>,
    pub jump_username: Option<String>,
    pub has_jump_private_key_path: bool,
    pub has_jump_certificate_path: bool,
    pub has_jump_password: bool,
    pub last_active_at_unix_ms: Option<i64>,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

impl PresentedRemoteConnection {
    pub(super) fn from_record(record: LocalRemoteConnection, flags: RemoteCredentialFlags) -> Self {
        Self {
            id: record.connection_id,
            name: record.name,
            host: record.host,
            port: record.port,
            username: record.username,
            authentication_type: record.authentication_type,
            has_password: flags.has_password,
            has_private_key_path: flags.has_private_key_path,
            has_certificate_path: flags.has_certificate_path,
            default_remote_path: record.default_remote_path,
            host_key_policy: record.host_key_policy,
            jump_enabled: record.jump_enabled,
            jump_connection_id: record.jump_connection_id,
            jump_host: record.jump_host,
            jump_port: record.jump_port,
            jump_username: record.jump_username,
            has_jump_private_key_path: flags.has_jump_private_key_path,
            has_jump_certificate_path: flags.has_jump_certificate_path,
            has_jump_password: flags.has_jump_password,
            last_active_at_unix_ms: record.last_active_at_unix_ms,
            version: record.version,
            created_at_unix_ms: record.created_at_unix_ms,
            updated_at_unix_ms: record.updated_at_unix_ms,
        }
    }
}
