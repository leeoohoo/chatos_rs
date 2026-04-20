use crate::domain::meta::{AuthMode, DbType, DbTypeCapabilities, DbTypeDescriptor, NetworkMode};

pub fn all() -> Vec<DbTypeDescriptor> {
    vec![
        postgres_descriptor(),
        mysql_descriptor(),
        sqlite_descriptor(),
        sql_server_descriptor(),
        oracle_descriptor(),
        mongodb_descriptor(),
    ]
}

fn postgres_descriptor() -> DbTypeDescriptor {
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

fn mysql_descriptor() -> DbTypeDescriptor {
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

fn sqlite_descriptor() -> DbTypeDescriptor {
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

fn sql_server_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::SqlServer,
        label: "SQL Server".to_string(),
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
            supports_materialized_view: false,
            supports_synonym: true,
            supports_package: false,
            supports_trigger: true,
        },
    }
}

fn oracle_descriptor() -> DbTypeDescriptor {
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

fn mongodb_descriptor() -> DbTypeDescriptor {
    DbTypeDescriptor {
        db_type: DbType::MongoDb,
        label: "MongoDB".to_string(),
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
            supports_trigger: false,
        },
    }
}
