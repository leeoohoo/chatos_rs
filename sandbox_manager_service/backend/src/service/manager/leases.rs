// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::http::StatusCode;
use chatos_sandbox_contract::{
    legacy_policy_permission_snapshot, ApprovalPolicy, ApprovalReviewer, EffectiveSandboxPolicy,
    PermissionProfileId, SandboxBackendKind, SandboxLeasePolicyRequest,
};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::auth::{SandboxAuthContext, SCOPE_LEASE_DESTROY, SCOPE_LEASE_READ, SCOPE_LEASE_RELEASE};
use crate::backend::SandboxCreateSpec;
use crate::config::{AppConfig, SandboxBackendKind as ManagerBackendKind};
use crate::error::ApiError;
use crate::models::{
    CreateSandboxLeaseRequest, CreateSandboxLeaseResponse, DestroySandboxResponse,
    HeartbeatRequest, HeartbeatResponse, ListSandboxQuery, NetworkPolicy, ReleaseSandboxRequest,
    ReleaseSandboxResponse, SandboxEventRecord, SandboxLeaseRecord, SandboxStatus,
};
use crate::store::is_duplicate_key_error;

use super::super::{images, output_manifest};
use super::lease_inputs::{normalize_idempotency_key, sanitize_path_segment, validate_required};
use super::{now_rfc3339, prefixed_id, SandboxManager};

mod create;
mod lifecycle;
pub(in crate::service::manager) mod policy;
mod queries;

use policy::*;

#[cfg(test)]
mod tests;
