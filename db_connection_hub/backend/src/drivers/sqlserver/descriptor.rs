use crate::domain::meta::{AuthMode, DbType, DbTypeCapabilities, DbTypeDescriptor, NetworkMode};

pub fn sqlserver_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::SqlServer,
        label: "SQL Server".to_string(),
        auth_modes: vec![AuthMode::Password, AuthMode::TlsClientCert],
        network_modes: vec![
            NetworkMode::Direct,
            NetworkMode::SshTunnel,
            NetworkMode::Proxy,
        ],
        capabilities: DbTypeCapabilities {
            has_database_level: true,
            has_schema_level: true,
            supports_materialized_view: false,
            supports_synonym: true,
            supports_package: false,
            supports_trigger: true,
        },
    }
}
