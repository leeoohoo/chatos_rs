use crate::domain::meta::{AuthMode, DbType, DbTypeCapabilities, DbTypeDescriptor, NetworkMode};

pub fn sqlite_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::Sqlite,
        label: "SQLite".to_string(),
        auth_modes: vec![AuthMode::NoAuth, AuthMode::FileKey],
        network_modes: vec![NetworkMode::Direct],
        capabilities: DbTypeCapabilities {
            has_database_level: false,
            has_schema_level: true,
            supports_materialized_view: false,
            supports_synonym: false,
            supports_package: false,
            supports_trigger: true,
        },
    }
}
