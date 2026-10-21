#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::{domain::StoryProject, projection::StoryProjectSnapshot};

pub(super) const STORY_PLANNING_RUN_VERSION: u32 = 1;
pub(super) const MAXIMUM_USER_IDEAS_CHARACTERS: usize = 2_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum StoryPlanningStage {
    Outline,
    Refine,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum StoryPlanningStatus {
    Ready,
    Running,
    Paused,
    Completed,
    Failed,
    NeedsReview,
    LimitReached,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentContextPolicy {
    pub window_tokens: u32,
    pub output_reserve_tokens: u32,
}

impl Default for StoryAgentContextPolicy {
    fn default() -> Self {
        Self {
            window_tokens: 2_000_000,
            output_reserve_tokens: 30_000,
        }
    }
}

impl StoryAgentContextPolicy {
    fn validate(&self) -> bool {
        (2_048..=2_000_000).contains(&self.window_tokens)
            && (256..self.window_tokens).contains(&self.output_reserve_tokens)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentRunPolicy {
    pub maximum_model_calls: u32,
    pub request_timeout_seconds: u32,
    pub run_timeout_seconds: u32,
    pub maximum_request_retries: u8,
    pub maximum_no_progress_rounds: u8,
    pub context: StoryAgentContextPolicy,
}

impl Default for StoryAgentRunPolicy {
    fn default() -> Self {
        Self {
            maximum_model_calls: 600,
            request_timeout_seconds: 180,
            run_timeout_seconds: 7_200,
            maximum_request_retries: 2,
            maximum_no_progress_rounds: 8,
            context: StoryAgentContextPolicy::default(),
        }
    }
}

impl StoryAgentRunPolicy {
    fn validate(&self) -> bool {
        (1..=10_000).contains(&self.maximum_model_calls)
            && (5..=1_800).contains(&self.request_timeout_seconds)
            && (10..=86_400).contains(&self.run_timeout_seconds)
            && self.maximum_request_retries <= 10
            && (1..=100).contains(&self.maximum_no_progress_rounds)
            && self.context.validate()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum StoryAgentMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentUsage {
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    pub requests: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentMessage {
    pub role: StoryAgentMessageRole,
    pub content: String,
    pub tool_calls: Vec<StoryAgentToolCall>,
    pub tool_call_id: Option<String>,
    /// Native-only exact Responses output. It must never enter the safe run snapshot.
    pub response_output_json: Option<Vec<u8>>,
    pub usage: Option<StoryAgentUsage>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentToolOutcome {
    pub content: String,
    pub made_progress: bool,
    pub is_error: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentInstructionBundle {
    pub name: String,
    pub version: u32,
    pub content_sha256: String,
    pub language: String,
    pub audience: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentMemoryScope {
    pub tenant_hash: String,
    pub source_id: String,
    pub thread_id: String,
    pub subject_id: String,
    pub run_id: String,
    pub runtime_scope: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentMemoryCheckpoint {
    pub scope: StoryAgentMemoryScope,
    pub record_epoch_unix_ms: u64,
    pub pinned_message_count: usize,
    pub synced_message_count: usize,
    pub synced_digest: String,
    pub thread_created: bool,
    pub sync_in_flight_end: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentCheckpoint {
    pub id: String,
    pub scope: String,
    pub messages: Vec<StoryAgentMessage>,
    pub pending_calls: Vec<StoryAgentToolCall>,
    pub in_flight_call_id: Option<String>,
    pub receipts: HashMap<String, StoryAgentToolOutcome>,
    pub call_fingerprints: HashMap<String, String>,
    pub observations: HashMap<String, String>,
    pub model_calls: u32,
    pub no_progress_rounds: u32,
    pub elapsed_seconds: f64,
    pub status: StoryPlanningStatus,
    pub result: Option<String>,
    pub completion_result: Option<String>,
    pub stop_reason: Option<String>,
    pub memory: Option<StoryAgentMemoryCheckpoint>,
    pub usage: Option<StoryAgentUsage>,
    pub instruction_bundles: Vec<StoryAgentInstructionBundle>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryAgentReceipt {
    pub name: String,
    pub arguments: String,
    pub outcome: StoryAgentToolOutcome,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryPlanningEvent {
    pub id: String,
    pub date_unix_ms: u64,
    pub kind: String,
    pub detail: String,
    pub model_calls: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryPlanningRun {
    pub version: u32,
    pub owner_hash: String,
    pub project_id: String,
    pub stage: StoryPlanningStage,
    pub target_ids: Vec<String>,
    pub base_project_fingerprint: String,
    pub uses_cloud_memory: bool,
    pub consent_at_unix_ms: u64,
    pub policy: StoryAgentRunPolicy,
    pub checkpoint: StoryAgentCheckpoint,
    pub draft: StoryProject,
    pub read_through: usize,
    pub tool_receipts: HashMap<String, StoryAgentReceipt>,
    pub events: Vec<StoryPlanningEvent>,
    pub applied: bool,
    pub abandoned_at_unix_ms: Option<u64>,
    pub updated_at_unix_ms: u64,
}

impl StoryPlanningRun {
    pub(super) fn new(
        project: &StoryProject,
        owner_user_id: &str,
        stage: StoryPlanningStage,
        target_ids: Vec<String>,
        user_ideas: &str,
        policy: StoryAgentRunPolicy,
        system_prompt: String,
        goal_prompt: impl FnOnce(&str) -> String,
        now_unix_ms: u64,
    ) -> Result<Self, String> {
        project.validate()?;
        if !policy.validate() || project.source.trim().is_empty() {
            return Err(planning_error());
        }
        let owner_hash = owner_hash(owner_user_id)?;
        let mut draft = project.clone();
        validate_targets_and_prepare_draft(&mut draft, stage, &target_ids)?;
        let user_ideas = user_ideas
            .trim()
            .chars()
            .take(MAXIMUM_USER_IDEAS_CHARACTERS)
            .collect::<String>();
        let run_id = Uuid::new_v4().to_string();
        let base_project_fingerprint = project_fingerprint(project)?;
        let scope = format!(
            "story:{owner_hash}:{}:{base_project_fingerprint}:{}",
            project.id,
            Uuid::new_v4()
        );
        let run = Self {
            version: STORY_PLANNING_RUN_VERSION,
            owner_hash,
            project_id: project.id.clone(),
            stage,
            target_ids,
            base_project_fingerprint,
            // Windows does not claim cloud Memory participation until the Local Agent Host
            // exposes the same owner-scoped Story memory contract used by macOS.
            uses_cloud_memory: false,
            consent_at_unix_ms: now_unix_ms,
            policy,
            checkpoint: StoryAgentCheckpoint {
                id: run_id,
                scope,
                messages: vec![
                    StoryAgentMessage {
                        role: StoryAgentMessageRole::System,
                        content: system_prompt,
                        tool_calls: Vec::new(),
                        tool_call_id: None,
                        response_output_json: None,
                        usage: None,
                    },
                    StoryAgentMessage {
                        role: StoryAgentMessageRole::User,
                        content: goal_prompt(&user_ideas),
                        tool_calls: Vec::new(),
                        tool_call_id: None,
                        response_output_json: None,
                        usage: None,
                    },
                ],
                pending_calls: Vec::new(),
                in_flight_call_id: None,
                receipts: HashMap::new(),
                call_fingerprints: HashMap::new(),
                observations: HashMap::new(),
                model_calls: 0,
                no_progress_rounds: 0,
                elapsed_seconds: 0.0,
                status: StoryPlanningStatus::Ready,
                result: None,
                completion_result: None,
                stop_reason: None,
                memory: None,
                usage: None,
                instruction_bundles: Vec::new(),
            },
            draft,
            read_through: 0,
            tool_receipts: HashMap::new(),
            events: Vec::new(),
            applied: false,
            abandoned_at_unix_ms: None,
            updated_at_unix_ms: now_unix_ms,
        };
        run.validate(&run.owner_hash, &run.project_id)?;
        Ok(run)
    }

    pub(super) fn id(&self) -> &str {
        &self.checkpoint.id
    }

    pub(super) fn validate(
        &self,
        expected_owner_hash: &str,
        expected_project_id: &str,
    ) -> Result<(), String> {
        if self.version != STORY_PLANNING_RUN_VERSION
            || !valid_hash(&self.owner_hash)
            || self.owner_hash != expected_owner_hash
            || self.project_id != expected_project_id
            || self.draft.id != expected_project_id
            || Uuid::parse_str(&self.checkpoint.id)
                .map(|value| value.to_string() != self.checkpoint.id)
                .unwrap_or(true)
            || self.base_project_fingerprint.len() != 64
            || !valid_hash(&self.base_project_fingerprint)
            || self.applied && self.abandoned_at_unix_ms.is_some()
            || self.read_through > self.draft.source.chars().count()
            || self.events.len() > 20_000
            || self.checkpoint.messages.len() > 20_000
            || self.checkpoint.pending_calls.len() > 1_000
            || self.tool_receipts.len() > 20_000
            || !self.policy.validate()
            || !self.checkpoint.elapsed_seconds.is_finite()
            || self.checkpoint.elapsed_seconds < 0.0
        {
            return Err(planning_error());
        }
        self.draft.validate()?;
        Ok(())
    }

    pub(super) fn snapshot(&self, owner_user_id: &str) -> StoryPlanningRunSnapshot {
        StoryPlanningRunSnapshot {
            id: self.checkpoint.id.clone(),
            owner_user_id: owner_user_id.to_owned(),
            project_id: self.project_id.clone(),
            stage: self.stage,
            target_ids: self.target_ids.clone(),
            base_project_fingerprint: self.base_project_fingerprint.clone(),
            draft: StoryProjectSnapshot::from_project(&self.draft),
            events: self.events.clone(),
            status: self.checkpoint.status,
            stop_reason: self.checkpoint.stop_reason.clone(),
            model_calls: self.checkpoint.model_calls,
            uses_cloud_memory: self.uses_cloud_memory,
            applied: self.applied,
            abandoned_at_unix_ms: self.abandoned_at_unix_ms,
            updated_at_unix_ms: self.updated_at_unix_ms,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryPlanningRunSnapshot {
    id: String,
    owner_user_id: String,
    project_id: String,
    stage: StoryPlanningStage,
    target_ids: Vec<String>,
    base_project_fingerprint: String,
    draft: StoryProjectSnapshot,
    events: Vec<StoryPlanningEvent>,
    status: StoryPlanningStatus,
    stop_reason: Option<String>,
    model_calls: u32,
    uses_cloud_memory: bool,
    applied: bool,
    abandoned_at_unix_ms: Option<u64>,
    updated_at_unix_ms: u64,
}

pub(super) fn project_fingerprint(project: &StoryProject) -> Result<String, String> {
    let mut value = project.clone();
    value.updated_at_unix_ms = 0;
    value.models.supported_video_durations = None;
    let bytes = serde_json::to_vec(&value).map_err(|_| planning_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_targets_and_prepare_draft(
    draft: &mut StoryProject,
    stage: StoryPlanningStage,
    target_ids: &[String],
) -> Result<(), String> {
    match stage {
        StoryPlanningStage::Outline if draft.segments.is_empty() && target_ids.is_empty() => Ok(()),
        StoryPlanningStage::Outline => Err(planning_error()),
        StoryPlanningStage::Refine => {
            let unique = target_ids.iter().collect::<HashSet<_>>();
            if target_ids.is_empty() || unique.len() != target_ids.len() {
                return Err(planning_error());
            }
            for target_id in target_ids {
                let segment = draft
                    .segments
                    .iter_mut()
                    .find(|segment| segment.id == *target_id)
                    .ok_or_else(planning_error)?;
                if segment.attempt.is_some() && segment.video.is_none() {
                    return Err(planning_error());
                }
                if let Some(video) = segment.video.take() {
                    if !segment
                        .archived_videos
                        .iter()
                        .any(|value| value.job_id == video.job_id)
                    {
                        segment.archived_videos.push(video);
                    }
                }
                if let Some(attempt) = segment.attempt.take() {
                    if !segment
                        .previous_attempts
                        .iter()
                        .any(|value| value.id == attempt.id)
                    {
                        segment.previous_attempts.push(attempt);
                    }
                }
                segment.actual_video_last_frame_id = None;
                segment.error = None;
                segment.detail = None;
                segment.first_frames.confirmed_image_id = None;
                segment.last_frames.confirmed_image_id = None;
                segment.video_guidance_mode = super::domain::StoryVideoGuidanceMode::FirstFrame;
            }
            Ok(())
        }
    }
}

fn owner_hash(owner_user_id: &str) -> Result<String, String> {
    let owner = owner_user_id.trim();
    if owner.is_empty() || owner.len() > 1_024 {
        return Err(planning_error());
    }
    Ok(format!("{:x}", Sha256::digest(owner.as_bytes())))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn planning_error() -> String {
    "剧情规划运行记录无效，原文件已保留。".into()
}
