use crate::domain::meta::{AuthMode, DbType, DbTypeCapabilities, DbTypeDescriptor, NetworkMode};

pub fn mysql_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::MySql,
        label: "MySQL / MariaDB".to_string(),
        auth_modes: vec![AuthMode::Password, AuthMode::TlsClientCert, AuthMode::Token],
        network_modes: vec![
            NetworkMode::Direct,
            NetworkMode::SshTunnel,
            NetworkMode::Proxy,
        ],
        capabilities: DbTypeCapabilities {
            has_database_level: true,
            has_schema_level: false,
            supports_materialized_view: false,
            supports_synonym: false,
            supports_package: false,
            supports_trigger: true,
        },
    }
}
