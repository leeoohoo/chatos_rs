// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub const DEFAULT_SSH_PORT: i64 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshAuthType {
    Password,
    PrivateKey,
    PrivateKeyCert,
}

impl SshAuthType {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "password" => Some(Self::Password),
            "private_key" => Some(Self::PrivateKey),
            "private_key_cert" => Some(Self::PrivateKeyCert),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "private_key",
            Self::PrivateKeyCert => "private_key_cert",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyPolicy {
    Strict,
    AcceptNew,
}

impl HostKeyPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "strict" => Some(Self::Strict),
            "accept_new" => Some(Self::AcceptNew),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::AcceptNew => "accept_new",
        }
    }
}

pub fn is_valid_ssh_port(port: i64) -> bool {
    (1..=u16::MAX as i64).contains(&port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_ssh_config_values_parse_canonical_names() {
        assert_eq!(
            SshAuthType::parse(" password "),
            Some(SshAuthType::Password)
        );
        assert_eq!(
            SshAuthType::parse("private_key_cert"),
            Some(SshAuthType::PrivateKeyCert)
        );
        assert_eq!(HostKeyPolicy::parse("strict"), Some(HostKeyPolicy::Strict));
        assert_eq!(
            HostKeyPolicy::parse("accept_new"),
            Some(HostKeyPolicy::AcceptNew)
        );
        assert!(is_valid_ssh_port(22));
        assert!(!is_valid_ssh_port(0));
        assert!(!is_valid_ssh_port(65_536));
    }
}
