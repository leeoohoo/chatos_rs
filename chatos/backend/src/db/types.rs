// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[derive(Clone)]
pub struct Database {
    pub pool: chatos_postgres::PgPool,
}

impl Database {
    pub fn pool(&self) -> &chatos_postgres::PgPool {
        &self.pool
    }
}
