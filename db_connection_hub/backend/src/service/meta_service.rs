// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::domain::meta::DbTypeListResponse;
use crate::drivers::registry::DriverRegistry;
use std::sync::Arc;

pub struct MetaService {
    registry: Arc<DriverRegistry>,
}

impl MetaService {
    pub fn new(registry: Arc<DriverRegistry>) -> Self {
        Self { registry }
    }

    pub fn list_db_types(&self) -> DbTypeListResponse {
        DbTypeListResponse {
            items: self.registry.descriptors(),
        }
    }
}
