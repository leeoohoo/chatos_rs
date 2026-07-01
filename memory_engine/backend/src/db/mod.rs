// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod connection;
mod index_helpers;
mod schema;

use mongodb::Database;

pub type Db = Database;

pub use self::connection::init_pool;
pub use self::schema::init_schema;
