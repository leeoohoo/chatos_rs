mod factory;
mod mongodb;
mod sqlite;
mod types;

#[cfg(test)]
pub use factory::get_factory;
pub use factory::{get_db, get_db_sync, init_global};
pub use types::Database;
