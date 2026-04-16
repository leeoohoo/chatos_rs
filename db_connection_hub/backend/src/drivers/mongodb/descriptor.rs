use crate::domain::meta::{AuthMode, DbType, DbTypeCapabilities, DbTypeDescriptor, NetworkMode};

pub fn mongodb_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::MongoDb,
        label: "MongoDB".to_string(),
        auth_modes: vec![
            AuthMode::NoAuth,
            AuthMode::Password,
            AuthMode::Token,
            AuthMode::TlsClientCert,
        ],
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
            supports_trigger: false,
        },
    }
}
