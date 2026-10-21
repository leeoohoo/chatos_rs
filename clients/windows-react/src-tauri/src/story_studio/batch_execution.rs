#![cfg_attr(not(test), allow(dead_code))]

use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::{
    authentication::AuthenticationManager,
    media_studio::{MediaStudioManager, StoryVideoPollOutcome},
};

use super::{
    application::{
        StoryAssetImageGenerationInput, StoryFrameImageGenerationInput, StoryFrameRoleInput,
        StoryVideoGenerationPollInput, StoryVideoGenerationStartInput,
    },
    batch::{
        StoryMediaBatchEvent, StoryMediaBatchKind, StoryMediaBatchRecord, StoryMediaBatchSnapshot,
        StoryMediaBatchStatus, StoryMediaBatchStoredStep,
    },
    domain::{StoryImageCollection, StoryProject},
    media::StoryStudioManager,
    planning::project_fingerprint,
    projection::StoryProjectSnapshot,
};

const STORY_MEDIA_BATCH_EVENT: &str = "story-media-batch";
const VIDEO_POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryMediaBatchExecutionOutcome {
    pub batch: StoryMediaBatchSnapshot,
    pub project: StoryProjectSnapshot,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoryMediaBatchEventEnvelope {
    presentation_id: String,
    batch: StoryMediaBatchSnapshot,
    project: StoryProjectSnapshot,
}

pub(super) struct StoryMediaBatchRuntime<'a> {
    pub authentication: &'a AuthenticationManager,
    pub media_manager: &'a MediaStudioManager,
    pub story_manager: &'a StoryStudioManager,
    pub app: &'a AppHandle,
    pub owner_user_id: &'a str,
    pub authentication_generation: u64,
    pub presentation_id: &'a str,
}

impl StoryMediaBatchRuntime<'_> {
    pub(super) async fn resume(
        &self,
        project_id: &str,
        batch_id: &str,
    ) -> Result<StoryMediaBatchExecutionOutcome, String> {
        self.require_current().await?;
        let _lease = self.story_manager.batch_executions.begin(
            self.owner_user_id,
            self.authentication_generation,
            project_id,
            batch_id,
        )?;
        let mut batch = self.load_batch(project_id, batch_id).await?;
        if batch.is_finished() {
            return Err("制作批次已经结束，不能再次恢复。".into());
        }
        if self.reconcile_interrupted_project(&mut batch).await? {
            let outcome = self.persist_state(&mut batch).await?;
            if batch.status == StoryMediaBatchStatus::NeedsReview {
                return Ok(outcome);
            }
        } else {
            self.commit_existing(&mut batch).await?;
        }

        batch.status = StoryMediaBatchStatus::Running;
        batch.error = None;
        push_event(&mut batch, "批量制作已开始或恢复");
        self.persist_state(&mut batch).await?;

        let execution = self.run_steps(&mut batch).await;
        match execution {
            Ok(()) => {
                batch.status = StoryMediaBatchStatus::Completed;
                batch.error = None;
                push_event(&mut batch, "本批制作完成");
                self.persist_state(&mut batch).await
            }
            Err(error) => {
                if self.require_current().await.is_err() {
                    return Err(error);
                }
                batch.status = if batch
                    .steps
                    .iter()
                    .any(|step| !step.completed && step.generation_intent_id.is_some())
                {
                    StoryMediaBatchStatus::NeedsReview
                } else {
                    StoryMediaBatchStatus::Paused
                };
                batch.error = Some(bounded_text(&error, 2_000));
                push_event(&mut batch, &error);
                self.persist_state(&mut batch).await
            }
        }
    }

    async fn run_steps(&self, batch: &mut StoryMediaBatchRecord) -> Result<(), String> {
        for index in 0..batch.steps.len() {
            self.require_current().await?;
            if batch.steps[index].completed {
                continue;
            }
            let step = batch.steps[index].clone();
            match step.kind {
                StoryMediaBatchKind::Assets
                | StoryMediaBatchKind::Frames
                | StoryMediaBatchKind::LastFrames => {
                    self.execute_image_step(batch, index, &step).await?
                }
                StoryMediaBatchKind::Videos => self.execute_video_step(batch, index, &step).await?,
                StoryMediaBatchKind::Pipeline => {
                    return Err("批量制作步骤无效。".into());
                }
            }
        }
        Ok(())
    }

    async fn execute_image_step(
        &self,
        batch: &mut StoryMediaBatchRecord,
        index: usize,
        step: &StoryMediaBatchStoredStep,
    ) -> Result<(), String> {
        let generation = self.authentication_generation;
        if let Some(intent_id) = step.generation_intent_id.as_deref() {
            let project = self.load_project(&batch.project_id).await?;
            let collection = image_collection(&project, step)
                .ok_or_else(|| "剧情图片目标已经变化。".to_owned())?;
            if collection
                .images
                .iter()
                .any(|image| image.generation_attempt_id.as_deref() == Some(intent_id))
            {
                batch.steps[index].completed = true;
                batch.steps[index].progress_status = Some("completed".into());
                push_event(batch, &format!("图片已保存：{}", step.target_id));
                self.adopt_known_project(batch).await?;
                return Ok(());
            }
            return Err("图片提交已经存在，但结果尚未可靠保存；请先核对服务商记录。".into());
        }

        let project = self.load_project(&batch.project_id).await?;
        let base_fingerprint = project_fingerprint(&project)?;
        let user_ideas = batch.user_ideas.clone().unwrap_or_default();
        let intent = match step.kind {
            StoryMediaBatchKind::Assets => {
                let manager = self.story_manager.clone();
                let owner = self.owner_user_id.to_owned();
                let presentation = self.presentation_id.to_owned();
                let request = StoryAssetImageGenerationInput {
                    project_id: batch.project_id.clone(),
                    base_fingerprint,
                    resource_id: step.target_id.clone(),
                    user_ideas,
                };
                ImageIntent::Asset(
                    tauri::async_runtime::spawn_blocking(move || {
                        manager.begin_asset_image_generation(
                            &owner,
                            generation,
                            &presentation,
                            request,
                        )
                    })
                    .await
                    .map_err(|_| "剧情素材图片生成准备任务异常退出。".to_owned())??,
                )
            }
            StoryMediaBatchKind::Frames | StoryMediaBatchKind::LastFrames => {
                let segment = project
                    .segments
                    .iter()
                    .find(|segment| segment.id == step.target_id)
                    .ok_or_else(|| "剧情分段已不存在。".to_owned())?;
                let references = segment.resource_ids().cloned().collect();
                let manager = self.story_manager.clone();
                let owner = self.owner_user_id.to_owned();
                let presentation = self.presentation_id.to_owned();
                let request = StoryFrameImageGenerationInput {
                    project_id: batch.project_id.clone(),
                    base_fingerprint,
                    segment_id: step.target_id.clone(),
                    frame_role: if step.kind == StoryMediaBatchKind::Frames {
                        StoryFrameRoleInput::First
                    } else {
                        StoryFrameRoleInput::Last
                    },
                    reference_resource_ids: references,
                    user_ideas,
                };
                ImageIntent::Frame(
                    tauri::async_runtime::spawn_blocking(move || {
                        manager.begin_frame_image_generation(
                            &owner,
                            generation,
                            &presentation,
                            request,
                        )
                    })
                    .await
                    .map_err(|_| "剧情首尾帧生成准备任务异常退出。".to_owned())??,
                )
            }
            _ => return Err("批量图片步骤无效。".into()),
        };
        self.require_current().await?;
        batch.steps[index].generation_intent_id = Some(intent.attempt_id().to_owned());
        batch.steps[index].progress_status = Some("generating".into());
        push_event(batch, &format!("生成图片：{}", step.target_id));
        self.adopt_known_project(batch).await?;

        let generated = self
            .media_manager
            .generate_story_image(
                self.authentication,
                self.owner_user_id,
                generation,
                intent.model_config_id(),
                intent.prompt().to_owned(),
                intent.attempt_id(),
                intent.references(),
            )
            .await?;
        self.require_current().await?;
        let manager = self.story_manager.clone();
        let owner = self.owner_user_id.to_owned();
        let presentation = self.presentation_id.to_owned();
        let provider_result_id = generated.provider_result_id;
        let provider_asset_id = generated.provider_asset_id;
        let bytes = generated.bytes;
        match intent {
            ImageIntent::Asset(intent) => {
                tauri::async_runtime::spawn_blocking(move || {
                    manager.complete_asset_image_generation(
                        &owner,
                        generation,
                        &presentation,
                        &intent.project_id,
                        &intent.resource_id,
                        &intent.attempt_id,
                        provider_result_id,
                        provider_asset_id,
                        &bytes,
                    )
                })
                .await
                .map_err(|_| "剧情素材图片保存任务异常退出。".to_owned())??;
            }
            ImageIntent::Frame(intent) => {
                tauri::async_runtime::spawn_blocking(move || {
                    manager.complete_frame_image_generation(
                        &owner,
                        generation,
                        &presentation,
                        &intent.project_id,
                        &intent.segment_id,
                        intent.frame_role,
                        &intent.attempt_id,
                        provider_result_id,
                        provider_asset_id,
                        &bytes,
                    )
                })
                .await
                .map_err(|_| "剧情首尾帧图片保存任务异常退出。".to_owned())??;
            }
        }
        self.require_current().await?;
        batch.steps[index].completed = true;
        batch.steps[index].progress_status = Some("completed".into());
        push_event(batch, &format!("图片已保存：{}", step.target_id));
        self.adopt_known_project(batch).await?;
        Ok(())
    }

    async fn execute_video_step(
        &self,
        batch: &mut StoryMediaBatchRecord,
        index: usize,
        step: &StoryMediaBatchStoredStep,
    ) -> Result<(), String> {
        let generation = self.authentication_generation;
        let mut project = self.load_project(&batch.project_id).await?;
        if let Some(segment) = project
            .segments
            .iter()
            .find(|segment| segment.id == step.target_id)
        {
            if let Some(video) = &segment.video {
                let known_job = batch.steps[index].provider_job_id.as_deref().or_else(|| {
                    segment
                        .attempt
                        .as_ref()
                        .and_then(|attempt| attempt.job_id.as_deref())
                });
                if known_job == Some(video.job_id.as_str()) {
                    batch.steps[index].provider_job_id = Some(video.job_id.clone());
                    batch.steps[index].completed = true;
                    batch.steps[index].progress_status = Some("completed".into());
                    push_event(batch, &format!("视频已保存：{}", segment.title));
                    self.adopt_known_project(batch).await?;
                    return Ok(());
                }
            }
        }

        if batch.steps[index].generation_intent_id.is_none() {
            let base_fingerprint = project_fingerprint(&project)?;
            let manager = self.story_manager.clone();
            let owner = self.owner_user_id.to_owned();
            let presentation = self.presentation_id.to_owned();
            let request = StoryVideoGenerationStartInput {
                project_id: batch.project_id.clone(),
                base_fingerprint,
                segment_id: step.target_id.clone(),
                size: "768P".into(),
                user_ideas: batch.user_ideas.clone().unwrap_or_default(),
            };
            let intent = tauri::async_runtime::spawn_blocking(move || {
                manager.begin_video_generation(&owner, generation, &presentation, request)
            })
            .await
            .map_err(|_| "剧情视频生成准备任务异常退出。".to_owned())??;
            self.require_current().await?;
            batch.steps[index].generation_intent_id = Some(intent.attempt_id.clone());
            batch.steps[index].progress_status = Some("submitting".into());
            push_event(batch, &format!("提交视频：{}", step.target_id));
            self.adopt_known_project(batch).await?;

            let submitted = self
                .media_manager
                .submit_story_video(
                    self.authentication,
                    self.owner_user_id,
                    generation,
                    intent.provider_request,
                )
                .await;
            self.require_current().await?;
            match submitted {
                Ok(job) => {
                    let manager = self.story_manager.clone();
                    let owner = self.owner_user_id.to_owned();
                    let presentation = self.presentation_id.to_owned();
                    let project_id = intent.project_id;
                    let segment_id = intent.segment_id;
                    let attempt_id = intent.attempt_id;
                    let job_id = job.job_id;
                    let saved_job_id = job_id.clone();
                    let status = job.status;
                    let saved_status = status.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        manager.record_video_submission(
                            &owner,
                            generation,
                            &presentation,
                            StoryVideoGenerationPollInput {
                                project_id,
                                segment_id,
                            },
                            &attempt_id,
                            &saved_job_id,
                            &saved_status,
                        )
                    })
                    .await
                    .map_err(|_| "剧情视频任务身份保存异常退出。".to_owned())??;
                    batch.steps[index].provider_job_id = Some(job_id);
                    batch.steps[index].progress_status = Some(status.clone());
                    push_event(batch, &format!("{}：{}", step.target_id, status));
                    self.adopt_known_project(batch).await?;
                }
                Err(error) => {
                    let manager = self.story_manager.clone();
                    let owner = self.owner_user_id.to_owned();
                    let presentation = self.presentation_id.to_owned();
                    let project_id = intent.project_id;
                    let segment_id = intent.segment_id;
                    let attempt_id = intent.attempt_id;
                    let message = error.message;
                    let saved_message = message.clone();
                    let before_submission = !error.request_may_have_been_submitted;
                    tauri::async_runtime::spawn_blocking(move || {
                        manager.record_video_failure(
                            &owner,
                            generation,
                            &presentation,
                            &project_id,
                            &segment_id,
                            &attempt_id,
                            &saved_message,
                            before_submission,
                        )
                    })
                    .await
                    .map_err(|_| "剧情视频失败状态保存异常退出。".to_owned())??;
                    batch.steps[index].progress_status = Some(
                        if before_submission {
                            "failed"
                        } else {
                            "submission_unknown"
                        }
                        .into(),
                    );
                    self.adopt_known_project(batch).await?;
                    return Err(message);
                }
            }
            project = self.load_project(&batch.project_id).await?;
        }

        let segment = project
            .segments
            .iter()
            .find(|segment| segment.id == step.target_id)
            .ok_or_else(|| "剧情分段已不存在。".to_owned())?;
        if batch.steps[index].provider_job_id.is_none() {
            let matching_attempt = segment.attempt.as_ref().filter(|attempt| {
                batch.steps[index].generation_intent_id.as_deref() == Some(attempt.id.as_str())
            });
            if let Some(job_id) = matching_attempt.and_then(|attempt| attempt.job_id.clone()) {
                batch.steps[index].provider_job_id = Some(job_id);
                self.adopt_known_project(batch).await?;
            } else {
                return Err("视频提交没有可靠的服务商任务 ID，请先核对账单。".into());
            }
        }

        loop {
            self.require_current().await?;
            let manager = self.story_manager.clone();
            let owner = self.owner_user_id.to_owned();
            let presentation = self.presentation_id.to_owned();
            let request = StoryVideoGenerationPollInput {
                project_id: batch.project_id.clone(),
                segment_id: step.target_id.clone(),
            };
            let intent = tauri::async_runtime::spawn_blocking(move || {
                manager.prepare_video_poll(&owner, generation, &presentation, request)
            })
            .await
            .map_err(|_| "剧情视频任务读取异常退出。".to_owned())??;
            let polled = self
                .media_manager
                .poll_story_video(
                    self.authentication,
                    self.owner_user_id,
                    generation,
                    &intent.attempt.model_config_id,
                    &intent.attempt.prompt,
                    &intent.attempt.size,
                    intent.attempt.seconds,
                    &intent.attempt.ratio,
                    &intent.attempt.id,
                    &intent.provider_job_id,
                )
                .await;
            self.require_current().await?;
            let manager = self.story_manager.clone();
            let owner = self.owner_user_id.to_owned();
            let presentation = self.presentation_id.to_owned();
            let project_id = intent.project_id;
            let segment_id = intent.segment_id;
            let attempt_id = intent.attempt.id;
            match polled {
                Ok(StoryVideoPollOutcome::Waiting {
                    job_id,
                    status,
                    percent: _,
                }) => {
                    let saved_job_id = job_id.clone();
                    let saved_status = status.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        manager.record_video_waiting(
                            &owner,
                            generation,
                            &presentation,
                            &project_id,
                            &segment_id,
                            &attempt_id,
                            &saved_job_id,
                            &saved_status,
                        )
                    })
                    .await
                    .map_err(|_| "剧情视频进度保存异常退出。".to_owned())??;
                    batch.steps[index].provider_job_id = Some(job_id);
                    if batch.steps[index].progress_status.as_deref() != Some(status.as_str()) {
                        batch.steps[index].progress_status = Some(status.clone());
                        push_event(batch, &format!("{}：{}", step.target_id, status));
                    }
                    self.adopt_known_project(batch).await?;
                    tokio::time::sleep(VIDEO_POLL_INTERVAL).await;
                }
                Ok(StoryVideoPollOutcome::Completed {
                    job_id,
                    model_name,
                    bytes,
                }) => {
                    let saved_job_id = job_id.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        manager.complete_video_generation(
                            &owner,
                            generation,
                            &presentation,
                            &project_id,
                            &segment_id,
                            &attempt_id,
                            saved_job_id,
                            model_name,
                            &bytes,
                        )
                    })
                    .await
                    .map_err(|_| "剧情视频保存任务异常退出。".to_owned())??;
                    batch.steps[index].provider_job_id = Some(job_id);
                    batch.steps[index].completed = true;
                    batch.steps[index].progress_status = Some("completed".into());
                    push_event(batch, &format!("视频已保存：{}", step.target_id));
                    self.adopt_known_project(batch).await?;
                    return Ok(());
                }
                Ok(StoryVideoPollOutcome::Failed { message }) | Err(message) => {
                    let saved_message = message.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        manager.record_video_failure(
                            &owner,
                            generation,
                            &presentation,
                            &project_id,
                            &segment_id,
                            &attempt_id,
                            &saved_message,
                            false,
                        )
                    })
                    .await
                    .map_err(|_| "剧情视频失败状态保存异常退出。".to_owned())??;
                    batch.steps[index].progress_status = Some("failed".into());
                    self.adopt_known_project(batch).await?;
                    return Err(message);
                }
            }
        }
    }

    async fn reconcile_interrupted_project(
        &self,
        batch: &mut StoryMediaBatchRecord,
    ) -> Result<bool, String> {
        let current = self.load_project(&batch.project_id).await?;
        let current_fingerprint = project_fingerprint(&current)?;
        if current_fingerprint == batch.expected_project_fingerprint
            || current_fingerprint == project_fingerprint(&batch.draft)?
        {
            return Ok(false);
        }
        for index in 0..batch.steps.len() {
            if batch.steps[index].completed {
                continue;
            }
            let step = batch.steps[index].clone();
            let exact_change = match step.kind {
                StoryMediaBatchKind::Assets
                | StoryMediaBatchKind::Frames
                | StoryMediaBatchKind::LastFrames => {
                    exact_image_target_change(&batch.draft, &current, &step)
                }
                StoryMediaBatchKind::Videos => {
                    exact_video_target_change(&batch.draft, &current, &step.target_id)
                }
                StoryMediaBatchKind::Pipeline => false,
            };
            if !exact_change {
                continue;
            }
            let known_intent = step.generation_intent_id.as_deref();
            let recoverable =
                match step.kind {
                    StoryMediaBatchKind::Assets
                    | StoryMediaBatchKind::Frames
                    | StoryMediaBatchKind::LastFrames => image_collection(&current, &step)
                        .is_some_and(|collection| {
                            known_intent.is_some_and(|intent| {
                                collection.images.iter().any(|image| {
                                    image.generation_attempt_id.as_deref() == Some(intent)
                                })
                            })
                        }),
                    StoryMediaBatchKind::Videos => current
                        .segments
                        .iter()
                        .find(|segment| segment.id == step.target_id)
                        .is_some_and(|segment| {
                            known_intent.is_some_and(|intent| {
                                segment.attempt.as_ref().is_some_and(|attempt| {
                                    attempt.id == intent
                                        && (attempt.job_id.is_some() || segment.video.is_some())
                                })
                            })
                        }),
                    StoryMediaBatchKind::Pipeline => false,
                };
            batch.draft = current;
            if recoverable {
                push_event(batch, "已恢复上次中断后保存的服务商检查点");
            } else {
                batch.status = StoryMediaBatchStatus::NeedsReview;
                let message = "检测到未能可靠归属的服务商提交，请核对任务或账单后再继续。";
                batch.error = Some(message.into());
                push_event(batch, message);
            }
            return Ok(true);
        }
        Err("项目已被修改，不能恢复旧制作批次。".into())
    }

    async fn load_batch(
        &self,
        project_id: &str,
        batch_id: &str,
    ) -> Result<StoryMediaBatchRecord, String> {
        let store = self.story_manager.store();
        let owner = self.owner_user_id.to_owned();
        let project = project_id.to_owned();
        let batch = batch_id.to_owned();
        tauri::async_runtime::spawn_blocking(move || {
            store.load_media_batch(&owner, &project, &batch)
        })
        .await
        .map_err(|_| "批量制作记录读取任务异常退出。".to_owned())?
    }

    async fn load_project(&self, project_id: &str) -> Result<StoryProject, String> {
        let store = self.story_manager.store();
        let owner = self.owner_user_id.to_owned();
        let project = project_id.to_owned();
        tauri::async_runtime::spawn_blocking(move || store.load_native_project(&owner, &project))
            .await
            .map_err(|_| "剧情项目读取任务异常退出。".to_owned())?
    }

    async fn commit_existing(&self, batch: &mut StoryMediaBatchRecord) -> Result<(), String> {
        let store = self.story_manager.store();
        let owner = self.owner_user_id.to_owned();
        let mut next = batch.clone();
        let committed = tauri::async_runtime::spawn_blocking(move || {
            store.commit_media_batch(&owner, &mut next)?;
            Ok::<_, String>(next)
        })
        .await
        .map_err(|_| "批量制作检查点恢复任务异常退出。".to_owned())??;
        *batch = committed;
        Ok(())
    }

    async fn adopt_known_project(
        &self,
        batch: &mut StoryMediaBatchRecord,
    ) -> Result<StoryMediaBatchExecutionOutcome, String> {
        batch.draft = self.load_project(&batch.project_id).await?;
        self.persist_state(batch).await
    }

    async fn persist_state(
        &self,
        batch: &mut StoryMediaBatchRecord,
    ) -> Result<StoryMediaBatchExecutionOutcome, String> {
        let generation = self.authentication_generation;
        self.require_current().await?;
        batch.updated_at_unix_ms = current_unix_milliseconds().max(batch.updated_at_unix_ms);
        let store = self.story_manager.store();
        let owner = self.owner_user_id.to_owned();
        let mut next = batch.clone();
        let committed = tauri::async_runtime::spawn_blocking(move || {
            store.commit_media_batch(&owner, &mut next)?;
            Ok::<_, String>(next)
        })
        .await
        .map_err(|_| "批量制作状态保存任务异常退出。".to_owned())??;
        *batch = committed;
        let manager = self.story_manager.clone();
        let owner = self.owner_user_id.to_owned();
        let presentation = self.presentation_id.to_owned();
        let project_id = batch.project_id.clone();
        let project = tauri::async_runtime::spawn_blocking(move || {
            manager.present_project(&owner, generation, &presentation, &project_id)
        })
        .await
        .map_err(|_| "剧情项目呈现任务异常退出。".to_owned())??;
        self.require_current().await?;
        let outcome = StoryMediaBatchExecutionOutcome {
            batch: batch.snapshot(self.owner_user_id),
            project,
        };
        self.app
            .emit(
                STORY_MEDIA_BATCH_EVENT,
                StoryMediaBatchEventEnvelope {
                    presentation_id: self.presentation_id.to_owned(),
                    batch: outcome.batch.clone(),
                    project: outcome.project.clone(),
                },
            )
            .map_err(|_| "批量制作状态发布失败。".to_owned())?;
        Ok(outcome)
    }

    async fn require_current(&self) -> Result<(), String> {
        self.authentication
            .require_session_generation(self.owner_user_id, self.authentication_generation)
            .await
    }
}

enum ImageIntent {
    Asset(super::media::StoryAssetImageGenerationIntent),
    Frame(super::media::StoryFrameImageGenerationIntent),
}

impl ImageIntent {
    fn attempt_id(&self) -> &str {
        match self {
            Self::Asset(value) => &value.attempt_id,
            Self::Frame(value) => &value.attempt_id,
        }
    }

    fn model_config_id(&self) -> &str {
        match self {
            Self::Asset(value) => &value.model_config_id,
            Self::Frame(value) => &value.model_config_id,
        }
    }

    fn prompt(&self) -> &str {
        match self {
            Self::Asset(value) => &value.prompt,
            Self::Frame(value) => &value.prompt,
        }
    }

    fn references(&self) -> Vec<crate::media_studio::StoryImageReference> {
        match self {
            Self::Asset(_) => Vec::new(),
            Self::Frame(value) => value.references.clone(),
        }
    }
}

fn image_collection<'a>(
    project: &'a StoryProject,
    step: &StoryMediaBatchStoredStep,
) -> Option<&'a StoryImageCollection> {
    match step.kind {
        StoryMediaBatchKind::Assets => project.resource_media(&step.target_id),
        StoryMediaBatchKind::Frames => project
            .segments
            .iter()
            .find(|segment| segment.id == step.target_id)
            .map(|segment| &segment.first_frames),
        StoryMediaBatchKind::LastFrames => project
            .segments
            .iter()
            .find(|segment| segment.id == step.target_id)
            .map(|segment| &segment.last_frames),
        _ => None,
    }
}

fn replace_image_collection(
    project: &mut StoryProject,
    step: &StoryMediaBatchStoredStep,
    collection: StoryImageCollection,
) -> bool {
    match step.kind {
        StoryMediaBatchKind::Assets => project
            .resource_media_mut(&step.target_id)
            .map(|value| *value = collection)
            .is_some(),
        StoryMediaBatchKind::Frames => project
            .segments
            .iter_mut()
            .find(|segment| segment.id == step.target_id)
            .map(|segment| segment.first_frames = collection)
            .is_some(),
        StoryMediaBatchKind::LastFrames => project
            .segments
            .iter_mut()
            .find(|segment| segment.id == step.target_id)
            .map(|segment| segment.last_frames = collection)
            .is_some(),
        _ => false,
    }
}

fn exact_image_target_change(
    draft: &StoryProject,
    current: &StoryProject,
    step: &StoryMediaBatchStoredStep,
) -> bool {
    let Some(original) = image_collection(draft, step).cloned() else {
        return false;
    };
    let mut normalized = current.clone();
    if !replace_image_collection(&mut normalized, step, original) {
        return false;
    }
    normalized.updated_at_unix_ms = draft.updated_at_unix_ms;
    normalized == *draft
}

fn exact_video_target_change(
    draft: &StoryProject,
    current: &StoryProject,
    target_id: &str,
) -> bool {
    let Some(original) = draft
        .segments
        .iter()
        .find(|segment| segment.id == target_id)
        .cloned()
    else {
        return false;
    };
    let mut normalized = current.clone();
    let Some(segment) = normalized
        .segments
        .iter_mut()
        .find(|segment| segment.id == target_id)
    else {
        return false;
    };
    *segment = original;
    normalized.updated_at_unix_ms = draft.updated_at_unix_ms;
    normalized == *draft
}

fn push_event(batch: &mut StoryMediaBatchRecord, detail: &str) {
    if batch.events.len() >= 20_000 {
        batch.events.remove(0);
    }
    batch.events.push(StoryMediaBatchEvent {
        id: Uuid::new_v4().to_string(),
        date_unix_ms: current_unix_milliseconds(),
        detail: bounded_text(detail, 1_000),
    });
}

fn bounded_text(value: &str, maximum: usize) -> String {
    let value = value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(maximum)
        .collect::<String>();
    if value.trim().is_empty() {
        "批量制作状态发生变化。".into()
    } else {
        value
    }
}

fn current_unix_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
