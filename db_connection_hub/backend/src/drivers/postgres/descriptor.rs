use crate::domain::meta::{AuthMode, DbType, DbTypeCapabilities, DbTypeDescriptor, NetworkMode};

pub fn postgres_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::Postgres,
        label: "PostgreSQL".to_string(),
        auth_modes: vec![
            AuthMode::Password,
            AuthMode::TlsClientCert,
            AuthMode::Token,
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
            supports_synonym: false,
            supports_package: false,
            supports_trigger: true,
        },
    }
}
