#![allow(dead_code)]

mod factory;
mod mongodb;
mod sqlite;
mod types;

#[allow(unused_imports)]
pub use factory::{get_db, get_db_sync, get_factory, init_global, DatabaseFactory};
#[allow(unused_imports)]
pub use types::{Database, DatabaseConfig, DatabaseType, MongoConfig, SqliteConfig};
