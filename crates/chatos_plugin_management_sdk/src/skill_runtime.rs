// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SKILL_RUNTIME_PROTOCOL_VERSION: u32 = 2;
pub const DEFAULT_SKILL_ACTIVATION_MAX_DEPTH: u32 = 8;
pub const DEFAULT_SKILL_ACTIVATION_LIMIT: u32 = 32;

include!("skill_runtime/metadata.rs");
include!("skill_runtime/descriptors.rs");
include!("skill_runtime/gate.rs");
include!("skill_runtime/document.rs");

#[cfg(test)]
mod tests;
