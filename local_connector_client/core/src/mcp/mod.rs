// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod http_runtime;
#[path = "config_provider.rs"]
pub(crate) mod provider;
pub(crate) mod selection;
#[path = "config_service.rs"]
pub(crate) mod service;
pub(crate) mod terminal;
#[path = "runtime_tools.rs"]
pub(crate) mod tools;
