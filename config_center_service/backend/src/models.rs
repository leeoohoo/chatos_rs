// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ROLE_SUPER_ADMIN: &str = "super_admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

impl CurrentUser {
    pub fn is_super_admin(&self) -> bool {
        self.role == ROLE_SUPER_ADMIN
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: CurrentUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDefinitionRecord {
    pub id: String,
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub scope: String,
    pub service_name: Option<String>,
    pub value_type: String,
    pub default_value: Value,
    #[serde(default)]
    pub nullable: bool,
    pub min: Option<i64>,
    pub max: Option<i64>,
    #[serde(default)]
    pub enum_options: Vec<String>,
    pub sensitivity: String,
    pub reload_mode: String,
    pub criticality: String,
    #[serde(default)]
    pub env_aliases: Vec<String>,
    pub owner_team: String,
    pub ui_order: i32,
    #[serde(default)]
    pub deprecated: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDraftRecord {
    pub id: String,
    pub environment: String,
    pub base_revision: i64,
    #[serde(default)]
    pub changes: BTreeMap<String, Value>,
    pub validation_status: String,
    #[serde(default)]
    pub validation_errors: Vec<String>,
    pub updated_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigReleaseRecord {
    pub id: String,
    pub environment: String,
    pub revision: i64,
    pub status: String,
    pub base_release_id: Option<String>,
    #[serde(default)]
    pub changed_keys: Vec<String>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    pub publish_message: String,
    pub created_by: String,
    pub created_at: String,
    pub published_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveReleaseRecord {
    pub id: String,
    pub environment: String,
    pub release_id: String,
    pub revision: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRecord {
    pub id: String,
    pub environment: Option<String>,
    pub action: String,
    pub actor_user_id: String,
    pub actor_display_name: String,
    pub release_id: Option<String>,
    #[serde(default)]
    pub changed_keys: Vec<String>,
    pub detail: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstanceRecord {
    pub id: String,
    pub environment: String,
    pub service_name: String,
    pub service_id: String,
    pub running_version: Option<String>,
    pub effective_revision: i64,
    pub effective_checksum: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub pending_restart_keys: Vec<String>,
    #[serde(default)]
    pub emergency_override_keys: Vec<String>,
    pub last_error: Option<String>,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftUpdateRequest {
    #[serde(default)]
    pub changes: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomDefinitionRequest {
    pub environment: String,
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub scope: String,
    pub service_name: Option<String>,
    pub value_type: String,
    pub default_value: Value,
    pub min: Option<i64>,
    pub max: Option<i64>,
    #[serde(default)]
    pub enum_options: Vec<String>,
    pub reload_mode: String,
    #[serde(default)]
    pub env_aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PublishRequest {
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    pub valid: bool,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveConfigResponse {
    pub environment: String,
    pub revision: i64,
    pub release_id: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceHeartbeatRequest {
    pub environment: String,
    pub service_name: String,
    pub service_id: String,
    pub running_version: Option<String>,
    pub effective_revision: i64,
    pub effective_checksum: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub pending_restart_keys: Vec<String>,
    #[serde(default)]
    pub emergency_override_keys: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
}
