use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum DbType {
    #[serde(rename = "postgres")]
    Postgres,
    #[serde(rename = "mysql")]
    MySql,
    #[serde(rename = "sqlite")]
    Sqlite,
    #[serde(rename = "sql_server")]
    SqlServer,
    #[serde(rename = "oracle")]
    Oracle,
    #[serde(rename = "mongodb")]
    MongoDb,
}

impl Display for DbType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            DbType::Postgres => "postgres",
            DbType::MySql => "mysql",
            DbType::Sqlite => "sqlite",
            DbType::SqlServer => "sql_server",
            DbType::Oracle => "oracle",
            DbType::MongoDb => "mongodb",
        };

        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    Password,
    TlsClientCert,
    Token,
    Integrated,
    FileKey,
    NoAuth,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    Direct,
    SshTunnel,
    Proxy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SslMode {
    Disabled,
    Preferred,
    Required,
    VerifyCa,
    VerifyFull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTypeCapabilities {
    pub has_database_level: bool,
    pub has_schema_level: bool,
    pub supports_materialized_view: bool,
    pub supports_synonym: bool,
    pub supports_package: bool,
    pub supports_trigger: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTypeDescriptor {
    pub db_type: DbType,
    pub label: String,
    pub auth_modes: Vec<AuthMode>,
    pub network_modes: Vec<NetworkMode>,
    pub capabilities: DbTypeCapabilities,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbTypeListResponse {
    pub items: Vec<DbTypeDescriptor>,
}
