// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod connection;
mod schema;

pub type Db = chatos_postgres::PgPool;

pub use self::connection::init_pool;
pub use self::schema::init_schema;
