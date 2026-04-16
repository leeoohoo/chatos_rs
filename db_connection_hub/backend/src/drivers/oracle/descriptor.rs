use crate::domain::meta::{AuthMode, DbType, DbTypeCapabilities, DbTypeDescriptor, NetworkMode};

pub fn oracle_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::Oracle,
        label: "Oracle".to_string(),
        auth_modes: vec![
            AuthMode::Password,
            AuthMode::TlsClientCert,
            AuthMode::FileKey,
            AuthMode::Integrated,
        ],
        network_modes: vec![
            NetworkMode::Direct,
            NetworkMode::SshTunnel,
            NetworkMode::Proxy,
        ],
        capabilities: DbTypeCapabilities {
            has_database_level: true,
            has_schema_level: true,
            supports_materialized_view: true,
            supports_synonym: true,
            supports_package: true,
            supports_trigger: true,
        },
    }
}
