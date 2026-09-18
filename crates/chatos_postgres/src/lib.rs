// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod config;
mod health;
mod metrics;
mod migration;
mod pool;

pub use config::PostgresConfig;
pub use health::check_health;
pub use metrics::render_pool_metrics;
pub use migration::{ensure_migrations_applied, run_migrations};
pub use pool::connect;
pub use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, thiserror::Error)]
pub enum PostgresError {
    #[error("invalid PostgreSQL configuration: {0}")]
    Configuration(String),
    #[error("PostgreSQL connection failed: {0}")]
    Connection(#[from] sqlx::Error),
    #[error("PostgreSQL migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("PostgreSQL schema is not current: {0}")]
    Schema(String),
}
