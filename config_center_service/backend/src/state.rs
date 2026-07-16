// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use mongodb::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use chatos_config_sdk::ConfigSnapshot;
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};

use crate::catalog::{
    builtin_definitions, LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS, USER_PREFERENCE_CONFIG_KEYS,
};
use crate::config::AppConfig;
use crate::models::{
    ActiveReleaseRecord, AuditEventRecord, ConfigDefinitionRecord, ConfigDraftRecord,
    ConfigReleaseRecord, CurrentUser, CustomDefinitionRequest, EffectiveConfigResponse,
    ServiceInstanceRecord, ValidationResponse,
};
use crate::store::AppStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: AppStore,
    http: reqwest::Client,
}

mod consul;
mod initialization;
mod maintenance;
mod releases;
mod support;
#[cfg(test)]
mod tests;

use self::support::*;
