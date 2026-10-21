#![cfg_attr(not(test), allow(dead_code))]

use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct StoryPlanningExecutionLease {
    pub request_id: String,
    pub owner_hash: String,
    pub authentication_generation: u64,
    pub presentation_id: String,
    pub project_id: String,
    pub run_id: String,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Default)]
pub(super) struct StoryPlanningExecutionRegistry {
    active: Arc<Mutex<Option<StoryPlanningExecutionLease>>>,
}

impl StoryPlanningExecutionRegistry {
    pub(super) fn begin(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<StoryPlanningExecutionLease, String> {
        let lease = StoryPlanningExecutionLease {
            request_id: Uuid::new_v4().to_string(),
            owner_hash: owner_hash(owner_user_id)?,
            authentication_generation,
            presentation_id: canonical_uuid(presentation_id)?,
            project_id: canonical_uuid(project_id)?,
            run_id: canonical_uuid(run_id)?,
            cancellation: CancellationToken::new(),
        };
        let mut active = self.active.lock().map_err(|_| execution_error())?;
        if active.is_some() {
            return Err("已有剧情规划正在运行，请等待当前运行结束或先将其暂停。".into());
        }
        *active = Some(lease.clone());
        Ok(lease)
    }

    pub(super) fn accepts(&self, lease: &StoryPlanningExecutionLease) -> bool {
        self.active
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .is_some_and(|active| {
                active.request_id == lease.request_id
                    && active.owner_hash == lease.owner_hash
                    && active.authentication_generation == lease.authentication_generation
                    && active.presentation_id == lease.presentation_id
                    && active.project_id == lease.project_id
                    && active.run_id == lease.run_id
                    && !active.cancellation.is_cancelled()
            })
    }

    pub(super) fn finish(&self, lease: &StoryPlanningExecutionLease) {
        if let Ok(mut active) = self.active.lock() {
            if active
                .as_ref()
                .is_some_and(|value| value.request_id == lease.request_id)
            {
                *active = None;
            }
        }
    }

    pub(super) fn cancel_presentation(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) {
        let Ok(owner_hash) = owner_hash(owner_user_id) else {
            return;
        };
        if let Ok(active) = self.active.lock() {
            if let Some(active) = active.as_ref().filter(|value| {
                value.owner_hash == owner_hash
                    && value.authentication_generation == authentication_generation
                    && value.presentation_id == presentation_id
            }) {
                active.cancellation.cancel();
            }
        }
    }

    pub(super) fn cancel_run(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        project_id: &str,
        run_id: &str,
    ) {
        let Ok(owner_hash) = owner_hash(owner_user_id) else {
            return;
        };
        if let Ok(active) = self.active.lock() {
            if let Some(active) = active.as_ref().filter(|value| {
                value.owner_hash == owner_hash
                    && value.authentication_generation == authentication_generation
                    && value.project_id == project_id
                    && value.run_id == run_id
            }) {
                active.cancellation.cancel();
            }
        }
    }

    pub(super) fn cancel_session(&self, owner_user_id: &str, authentication_generation: u64) {
        let Ok(owner_hash) = owner_hash(owner_user_id) else {
            return;
        };
        if let Ok(active) = self.active.lock() {
            if let Some(active) = active.as_ref().filter(|value| {
                value.owner_hash == owner_hash
                    && value.authentication_generation == authentication_generation
            }) {
                active.cancellation.cancel();
            }
        }
    }

    pub(super) fn reset(&self) {
        if let Ok(mut active) = self.active.lock() {
            if let Some(value) = active.take() {
                value.cancellation.cancel();
            }
        }
    }
}

fn owner_hash(owner_user_id: &str) -> Result<String, String> {
    let owner = owner_user_id.trim();
    if owner.is_empty() || owner.len() > 1_024 {
        return Err(execution_error());
    }
    Ok(format!("{:x}", Sha256::digest(owner.as_bytes())))
}

fn canonical_uuid(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value).map_err(|_| execution_error())?;
    let canonical = parsed.to_string();
    if canonical != value {
        return Err(execution_error());
    }
    Ok(canonical)
}

fn execution_error() -> String {
    "剧情规划执行身份无效，请重新打开项目。".into()
}
