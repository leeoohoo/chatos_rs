use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use uuid::Uuid;

use super::{
    history::owner_hash,
    video_history::VideoHistoryItem,
    video_provider::{ProviderVideoJob, ResolvedVideoModel, VideoGenerationRequest},
};

const MAXIMUM_ACTIVE_JOBS_PER_OWNER: usize = 4;
const MAXIMUM_RETAINED_JOBS: usize = 64;
const MAXIMUM_POLL_COUNT: u16 = 180;
const MAXIMUM_CLOSED_PRESENTATIONS: usize = 64;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaStudioVideoJobSnapshot {
    pub job_id: String,
    pub client_request_id: String,
    pub status: String,
    pub percent: Option<f64>,
    pub provider_job_id: Option<String>,
    pub result: Option<VideoHistoryItem>,
    pub error_message: Option<String>,
    pub terminal: bool,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct VideoJobScope {
    owner_hash: String,
    authentication_generation: u64,
    presentation_id: String,
}

struct VideoJobEntry {
    sequence: u64,
    scope: VideoJobScope,
    request: VideoGenerationRequest,
    runtime: ResolvedVideoModel,
    provider_job: ProviderVideoJob,
    snapshot: MediaStudioVideoJobSnapshot,
    poll_count: u16,
    in_flight: bool,
    lease: u64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct VideoJobState {
    sequence: u64,
    jobs: HashMap<String, VideoJobEntry>,
    closed_presentations: HashSet<VideoJobScope>,
}

#[derive(Clone)]
pub(super) struct MediaStudioVideoJobRegistry {
    state: Arc<Mutex<VideoJobState>>,
}

pub(super) enum VideoPollStart {
    Snapshot(Box<MediaStudioVideoJobSnapshot>),
    Work(Box<VideoPollWork>),
}

#[derive(Clone)]
pub(super) struct VideoPollWork {
    pub job_id: String,
    pub lease: u64,
    pub request: VideoGenerationRequest,
    pub runtime: ResolvedVideoModel,
    pub provider_job: ProviderVideoJob,
    pub cancelled: Arc<AtomicBool>,
}

impl Default for MediaStudioVideoJobRegistry {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(VideoJobState::default())),
        }
    }
}

impl MediaStudioVideoJobRegistry {
    pub(super) fn register(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        request: VideoGenerationRequest,
        runtime: ResolvedVideoModel,
        provider_job: ProviderVideoJob,
    ) -> Result<MediaStudioVideoJobSnapshot, String> {
        let scope = scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut state = self.state()?;
        if state.closed_presentations.contains(&scope) {
            return Err("视频创作页面已关闭，请重新打开后生成。".into());
        }
        let active_count = state
            .jobs
            .values()
            .filter(|entry| entry.scope.owner_hash == scope.owner_hash && !entry.snapshot.terminal)
            .count();
        if active_count >= MAXIMUM_ACTIVE_JOBS_PER_OWNER {
            return Err("当前账号正在生成的视频过多，请等待已有任务完成。".into());
        }
        prune_terminal_jobs(&mut state);
        if state.jobs.len() >= MAXIMUM_RETAINED_JOBS {
            return Err("当前视频生成任务过多，请稍后重试。".into());
        }
        state.sequence = state.sequence.wrapping_add(1);
        let sequence = state.sequence;
        let job_id = Uuid::new_v4().to_string();
        let failed = provider_job.is_failed();
        let snapshot = MediaStudioVideoJobSnapshot {
            job_id: job_id.clone(),
            client_request_id: request.client_request_id.clone(),
            status: if failed {
                "failed".into()
            } else {
                provider_job.status.clone()
            },
            percent: provider_job.progress,
            provider_job_id: Some(provider_job.id.clone()),
            result: None,
            error_message: failed.then(|| provider_failure_message(&provider_job)),
            terminal: failed,
            revision: 1,
        };
        state.jobs.insert(
            job_id,
            VideoJobEntry {
                sequence,
                scope,
                request,
                runtime,
                provider_job,
                snapshot: snapshot.clone(),
                poll_count: 0,
                in_flight: false,
                lease: 0,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(snapshot)
    }

    pub(super) fn begin_poll(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        job_id: &str,
    ) -> Result<VideoPollStart, String> {
        let expected = scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut state = self.state()?;
        let entry = scoped_entry_mut(&mut state, &expected, job_id)?;
        if entry.snapshot.terminal || entry.in_flight {
            return Ok(VideoPollStart::Snapshot(Box::new(entry.snapshot.clone())));
        }
        if entry.poll_count >= MAXIMUM_POLL_COUNT {
            entry.snapshot.status = "failed".into();
            entry.snapshot.error_message = Some("视频生成等待超时，可以稍后重新尝试。".into());
            entry.snapshot.terminal = true;
            entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
            return Ok(VideoPollStart::Snapshot(Box::new(entry.snapshot.clone())));
        }
        entry.poll_count += 1;
        entry.in_flight = true;
        entry.lease = entry.lease.wrapping_add(1);
        Ok(VideoPollStart::Work(Box::new(VideoPollWork {
            job_id: job_id.to_owned(),
            lease: entry.lease,
            request: entry.request.clone(),
            runtime: entry.runtime.clone(),
            provider_job: entry.provider_job.clone(),
            cancelled: Arc::clone(&entry.cancelled),
        })))
    }

    pub(super) fn update_waiting(
        &self,
        work: &VideoPollWork,
        provider_job: ProviderVideoJob,
    ) -> Result<MediaStudioVideoJobSnapshot, String> {
        let mut state = self.state()?;
        let entry = leased_entry_mut(&mut state, work)?;
        entry.provider_job = provider_job.clone();
        entry.snapshot.status = provider_job.status;
        entry.snapshot.percent = provider_job.progress;
        entry.snapshot.provider_job_id = Some(provider_job.id);
        entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
        entry.in_flight = false;
        Ok(entry.snapshot.clone())
    }

    pub(super) fn begin_download(
        &self,
        work: &VideoPollWork,
        provider_job: ProviderVideoJob,
    ) -> Result<MediaStudioVideoJobSnapshot, String> {
        let mut state = self.state()?;
        let entry = leased_entry_mut(&mut state, work)?;
        entry.provider_job = provider_job.clone();
        entry.snapshot.status = "downloading".into();
        entry.snapshot.percent = provider_job.progress.or(Some(100.0));
        entry.snapshot.provider_job_id = Some(provider_job.id);
        entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
        Ok(entry.snapshot.clone())
    }

    pub(super) fn begin_saving(
        &self,
        work: &VideoPollWork,
    ) -> Result<MediaStudioVideoJobSnapshot, String> {
        let mut state = self.state()?;
        let entry = leased_entry_mut(&mut state, work)?;
        entry.snapshot.status = "saving".into();
        entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
        Ok(entry.snapshot.clone())
    }

    pub(super) fn complete(
        &self,
        work: &VideoPollWork,
        result: VideoHistoryItem,
    ) -> Result<MediaStudioVideoJobSnapshot, String> {
        let mut state = self.state()?;
        let entry = leased_entry_mut(&mut state, work)?;
        entry.snapshot.status = "completed".into();
        entry.snapshot.percent = Some(100.0);
        entry.snapshot.result = Some(result);
        entry.snapshot.error_message = None;
        entry.snapshot.terminal = true;
        entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
        entry.in_flight = false;
        Ok(entry.snapshot.clone())
    }

    pub(super) fn fail(
        &self,
        work: &VideoPollWork,
        error_message: String,
    ) -> Result<MediaStudioVideoJobSnapshot, String> {
        let mut state = self.state()?;
        let entry = leased_entry_mut(&mut state, work)?;
        entry.snapshot.status = "failed".into();
        entry.snapshot.error_message = Some(bounded_error(error_message));
        entry.snapshot.terminal = true;
        entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
        entry.in_flight = false;
        Ok(entry.snapshot.clone())
    }

    pub(super) fn cancel(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
        job_id: &str,
    ) -> Result<(), String> {
        let expected = scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut state = self.state()?;
        let entry = scoped_entry_mut(&mut state, &expected, job_id)?;
        entry.cancelled.store(true, Ordering::Release);
        entry.snapshot.status = "cancelled".into();
        entry.snapshot.error_message = None;
        entry.snapshot.terminal = true;
        entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
        entry.in_flight = false;
        entry.lease = entry.lease.wrapping_add(1);
        Ok(())
    }

    pub(super) fn discard_presentation(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        presentation_id: &str,
    ) -> Result<(), String> {
        let expected = scope(owner_user_id, authentication_generation, presentation_id)?;
        let mut state = self.state()?;
        state.closed_presentations.insert(expected.clone());
        if state.closed_presentations.len() > MAXIMUM_CLOSED_PRESENTATIONS {
            state.closed_presentations.clear();
            state.closed_presentations.insert(expected.clone());
        }
        for entry in state
            .jobs
            .values_mut()
            .filter(|entry| entry.scope == expected)
        {
            entry.cancelled.store(true, Ordering::Release);
            entry.snapshot.status = "cancelled".into();
            entry.snapshot.error_message = None;
            entry.snapshot.terminal = true;
            entry.snapshot.revision = entry.snapshot.revision.wrapping_add(1);
            entry.in_flight = false;
            entry.lease = entry.lease.wrapping_add(1);
        }
        Ok(())
    }

    pub(super) fn reset(&self) {
        if let Ok(mut state) = self.state.lock() {
            for entry in state.jobs.values() {
                entry.cancelled.store(true, Ordering::Release);
            }
            state.jobs.clear();
            state.closed_presentations.clear();
            state.sequence = state.sequence.wrapping_add(1);
        }
    }

    fn state(&self) -> Result<std::sync::MutexGuard<'_, VideoJobState>, String> {
        self.state
            .lock()
            .map_err(|_| "视频生成任务状态不可用。".to_owned())
    }
}

fn scope(
    owner_user_id: &str,
    authentication_generation: u64,
    presentation_id: &str,
) -> Result<VideoJobScope, String> {
    let parsed = Uuid::parse_str(presentation_id).map_err(|_| "视频创作页面标识无效。")?;
    if parsed.to_string() != presentation_id {
        return Err("视频创作页面标识无效。".into());
    }
    Ok(VideoJobScope {
        owner_hash: owner_hash(owner_user_id),
        authentication_generation,
        presentation_id: presentation_id.to_owned(),
    })
}

fn scoped_entry_mut<'a>(
    state: &'a mut VideoJobState,
    expected: &VideoJobScope,
    job_id: &str,
) -> Result<&'a mut VideoJobEntry, String> {
    if !valid_job_id(job_id) {
        return Err("视频生成任务标识无效。".into());
    }
    state
        .jobs
        .get_mut(job_id)
        .filter(|entry| entry.scope == *expected)
        .ok_or_else(|| "视频生成任务不存在或已失效。".to_owned())
}

fn leased_entry_mut<'a>(
    state: &'a mut VideoJobState,
    work: &VideoPollWork,
) -> Result<&'a mut VideoJobEntry, String> {
    let entry = state
        .jobs
        .get_mut(&work.job_id)
        .filter(|entry| {
            entry.in_flight
                && entry.lease == work.lease
                && !entry.cancelled.load(Ordering::Acquire)
                && !entry.snapshot.terminal
        })
        .ok_or_else(|| "视频生成任务已取消或被替换。".to_owned())?;
    Ok(entry)
}

fn prune_terminal_jobs(state: &mut VideoJobState) {
    if state.jobs.len() < MAXIMUM_RETAINED_JOBS {
        return;
    }
    let mut terminal = state
        .jobs
        .iter()
        .filter(|(_, entry)| entry.snapshot.terminal)
        .map(|(id, entry)| (id.clone(), entry.sequence))
        .collect::<Vec<_>>();
    terminal.sort_by_key(|(_, sequence)| *sequence);
    let remove_count = state.jobs.len().saturating_sub(MAXIMUM_RETAINED_JOBS - 1);
    for (id, _) in terminal.into_iter().take(remove_count) {
        state.jobs.remove(&id);
    }
}

fn provider_failure_message(job: &ProviderVideoJob) -> String {
    format!(
        "视频生成失败：{}",
        job.error_message
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("模型未提供失败原因")
    )
}

fn bounded_error(value: String) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(2_000)
        .collect::<String>()
}

fn valid_job_id(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.to_string() == value)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Url;

    fn request(id: &str) -> VideoGenerationRequest {
        VideoGenerationRequest {
            model_config_id: "video-model".into(),
            prompt: "clip".into(),
            size: "768P".into(),
            seconds: 4,
            ratio: "16:9".into(),
            input_image: None,
            reference_audio: None,
            client_request_id: id.into(),
        }
    }

    fn runtime() -> ResolvedVideoModel {
        ResolvedVideoModel {
            model_config_id: "video-model".into(),
            model_name: "MiniMax-H3".into(),
            base_url: Url::parse("https://provider.example/v1").unwrap(),
            api_key: "secret".into(),
        }
    }

    fn provider_job(status: &str) -> ProviderVideoJob {
        ProviderVideoJob {
            id: "provider-job".into(),
            status: status.into(),
            progress: Some(10.0),
            model_name: Some("MiniMax-H3".into()),
            error_message: None,
            content_url: None,
        }
    }

    #[test]
    fn scope_rejects_other_owner_generation_and_presentation() {
        let registry = MediaStudioVideoJobRegistry::default();
        let presentation = Uuid::new_v4().to_string();
        let snapshot = registry
            .register(
                "alice",
                7,
                &presentation,
                request("request-1"),
                runtime(),
                provider_job("queued"),
            )
            .unwrap();
        assert!(registry
            .begin_poll("bob", 7, &presentation, &snapshot.job_id)
            .is_err());
        assert!(registry
            .begin_poll("alice", 8, &presentation, &snapshot.job_id)
            .is_err());
        assert!(registry
            .begin_poll("alice", 7, &Uuid::new_v4().to_string(), &snapshot.job_id,)
            .is_err());
    }

    #[test]
    fn concurrent_poll_returns_snapshot_until_lease_finishes() {
        let registry = MediaStudioVideoJobRegistry::default();
        let presentation = Uuid::new_v4().to_string();
        let snapshot = registry
            .register(
                "alice",
                1,
                &presentation,
                request("request-1"),
                runtime(),
                provider_job("queued"),
            )
            .unwrap();
        let VideoPollStart::Work(work) = registry
            .begin_poll("alice", 1, &presentation, &snapshot.job_id)
            .unwrap()
        else {
            panic!("first poll must own the lease")
        };
        let VideoPollStart::Snapshot(concurrent) = registry
            .begin_poll("alice", 1, &presentation, &snapshot.job_id)
            .unwrap()
        else {
            panic!("concurrent poll must not issue provider work")
        };
        assert_eq!(concurrent.revision, 1);
        let updated = registry
            .update_waiting(&work, provider_job("in_progress"))
            .unwrap();
        assert_eq!(updated.status, "in_progress");
        assert_eq!(updated.revision, 2);
    }

    #[test]
    fn cancellation_invalidates_in_flight_lease_and_late_completion() {
        let registry = MediaStudioVideoJobRegistry::default();
        let presentation = Uuid::new_v4().to_string();
        let snapshot = registry
            .register(
                "alice",
                1,
                &presentation,
                request("request-1"),
                runtime(),
                provider_job("queued"),
            )
            .unwrap();
        let VideoPollStart::Work(work) = registry
            .begin_poll("alice", 1, &presentation, &snapshot.job_id)
            .unwrap()
        else {
            panic!("poll must own the lease")
        };
        registry
            .cancel("alice", 1, &presentation, &snapshot.job_id)
            .unwrap();
        assert!(work.cancelled.load(Ordering::Acquire));
        assert!(registry
            .begin_download(&work, provider_job("completed"))
            .is_err());
    }

    #[test]
    fn completed_result_is_terminal_and_contains_no_native_secret() {
        let registry = MediaStudioVideoJobRegistry::default();
        let presentation = Uuid::new_v4().to_string();
        let snapshot = registry
            .register(
                "alice",
                1,
                &presentation,
                request("request-1"),
                runtime(),
                provider_job("completed"),
            )
            .unwrap();
        let VideoPollStart::Work(work) = registry
            .begin_poll("alice", 1, &presentation, &snapshot.job_id)
            .unwrap()
        else {
            panic!("poll must own the lease")
        };
        registry
            .begin_download(&work, provider_job("completed"))
            .unwrap();
        registry.begin_saving(&work).unwrap();
        let completed = registry
            .complete(
                &work,
                VideoHistoryItem {
                    id: Uuid::new_v4().to_string(),
                    prompt: "clip".into(),
                    model_name: "MiniMax-H3".into(),
                    created_at_unix_ms: 1,
                    mime_type: "video/mp4".into(),
                    content_token: "a".repeat(32),
                    preview_url: Some("chatos-media://localhost/token/record/content".into()),
                },
            )
            .unwrap();
        assert!(completed.terminal);
        assert_eq!(completed.status, "completed");
        let encoded = serde_json::to_string(&completed).unwrap();
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("provider.example"));
    }

    #[test]
    fn discarded_presentation_rejects_a_late_creation_response() {
        let registry = MediaStudioVideoJobRegistry::default();
        let presentation = Uuid::new_v4().to_string();
        registry
            .discard_presentation("alice", 1, &presentation)
            .unwrap();
        assert!(registry
            .register(
                "alice",
                1,
                &presentation,
                request("late-request"),
                runtime(),
                provider_job("queued"),
            )
            .is_err());
    }
}
