// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
use serde_json::{json, Value};

#[path = "catalog/builtin.rs"]
mod builtin;
#[path = "catalog/constants.rs"]
mod constants;

pub use builtin::builtin_definitions;
pub use constants::*;

#[cfg(test)]
mod tests;
