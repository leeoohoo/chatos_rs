#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use super::{
    batch::{StoryMediaBatchHistoryPage, StoryMediaBatchRecord, MAXIMUM_MEDIA_BATCH_BYTES},
    domain::{valid_storage_filename, StoryProject},
    planning::{project_fingerprint, StoryPlanningRun, StoryPlanningStatus},
    projection::StoryProjectSnapshot,
};

const MAXIMUM_PROJECT_BYTES: usize = 8 * 1_024 * 1_024;
const MAXIMUM_PLANNING_RUN_BYTES: usize = 64 * 1_024 * 1_024;
pub(super) const MAXIMUM_STORY_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
pub(super) const MAXIMUM_STORY_VIDEO_BYTES: usize = 512 * 1_024 * 1_024;
const DEFAULT_PROJECT_PAGE_SIZE: usize = 40;
const MAXIMUM_PROJECT_PAGE_SIZE: usize = 200;
const DEFAULT_HISTORY_PAGE_SIZE: usize = 20;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectCursor {
    pub modified_at_unix_ms: u64,
    pub record_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryProjectPage {
    pub items: Vec<StoryProjectSnapshot>,
    pub next_cursor: Option<StoryProjectCursor>,
    pub unreadable_count: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryPlanningHistoryPage {
    pub runs: Vec<super::planning::StoryPlanningRunSnapshot>,
    pub next_cursor: Option<StoryProjectCursor>,
    pub unreadable_count: usize,
    pub recovered_project: Option<StoryProjectSnapshot>,
}

pub(super) struct NativeStoryProjectPage {
    pub items: Vec<StoryProject>,
    pub next_cursor: Option<StoryProjectCursor>,
    pub unreadable_count: usize,
}

#[derive(Clone)]
pub(super) struct StoryProjectStore {
    root: PathBuf,
    mutation_lock: Arc<Mutex<()>>,
}

impl StoryProjectStore {
    pub(super) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        if !app_data_directory.exists() {
            fs::create_dir_all(&app_data_directory).map_err(|_| store_error())?;
        }
        validate_private_directory(&app_data_directory)?;
        let root = private_child_directory(&app_data_directory, "StoryStudio")?;
        Ok(Self {
            root,
            mutation_lock: Arc::new(Mutex::new(())),
        })
    }

    #[cfg(test)]
    pub(super) fn for_tests(app_data_directory: PathBuf) -> Result<Self, String> {
        Self::new(app_data_directory)
    }

    pub(super) fn save(&self, owner_user_id: &str, project: &StoryProject) -> Result<(), String> {
        let owner = validate_owner(owner_user_id)?;
        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        self.save_locked(owner, project)
    }

    pub(super) fn mutate_project(
        &self,
        owner_user_id: &str,
        project_id: &str,
        mutation: impl FnOnce(&mut StoryProject) -> Result<(), String>,
    ) -> Result<StoryProject, String> {
        let owner = validate_owner(owner_user_id)?;
        let project_id = canonical_project_id(project_id)?;
        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(store_error)?;
        let mut project = load_project_manifest(&projects.join(&project_id), &project_id)?;
        mutation(&mut project)?;
        self.save_locked(owner, &project)?;
        Ok(project)
    }

    pub(super) fn mutate_project_with_image(
        &self,
        owner_user_id: &str,
        project_id: &str,
        filename: &str,
        bytes: &[u8],
        mutation: impl FnOnce(&mut StoryProject) -> Result<(), String>,
    ) -> Result<StoryProject, String> {
        if !valid_storage_filename(filename)
            || !filename.ends_with(".png")
            || bytes.is_empty()
            || bytes.len() > MAXIMUM_STORY_IMAGE_BYTES
        {
            return Err(store_error());
        }
        let owner = validate_owner(owner_user_id)?;
        let project_id = canonical_project_id(project_id)?;
        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(store_error)?;
        let project_directory = projects.join(&project_id);
        let mut project = load_project_manifest(&project_directory, &project_id)?;
        let asset = project_directory.join(filename);
        if fs::symlink_metadata(&asset).is_ok() {
            return Err(store_error());
        }
        write_private_file(&asset, bytes)?;
        if let Err(error) = mutation(&mut project) {
            let _ = fs::remove_file(&asset);
            return Err(error);
        }
        if let Err(error) = self.save_locked(owner, &project) {
            if let Ok(persisted) = load_project_manifest(&project_directory, &project_id) {
                if project_references_media(
                    &persisted,
                    filename,
                    "image/png",
                    MAXIMUM_STORY_IMAGE_BYTES,
                ) {
                    return Ok(persisted);
                }
            }
            let _ = fs::remove_file(&asset);
            return Err(error);
        }
        Ok(project)
    }

    pub(super) fn mutate_project_with_video(
        &self,
        owner_user_id: &str,
        project_id: &str,
        filename: &str,
        bytes: &[u8],
        mutation: impl FnOnce(&mut StoryProject) -> Result<(), String>,
    ) -> Result<StoryProject, String> {
        if !valid_storage_filename(filename)
            || !filename.ends_with(".mp4")
            || bytes.is_empty()
            || bytes.len() > MAXIMUM_STORY_VIDEO_BYTES
        {
            return Err(store_error());
        }
        let owner = validate_owner(owner_user_id)?;
        let project_id = canonical_project_id(project_id)?;
        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(store_error)?;
        let project_directory = projects.join(&project_id);
        let mut project = load_project_manifest(&project_directory, &project_id)?;
        let asset = project_directory.join(filename);
        if fs::symlink_metadata(&asset).is_ok() {
            return Err(store_error());
        }
        write_private_file(&asset, bytes)?;
        if let Err(error) = mutation(&mut project) {
            let _ = fs::remove_file(&asset);
            return Err(error);
        }
        if let Err(error) = self.save_locked(owner, &project) {
            if let Ok(persisted) = load_project_manifest(&project_directory, &project_id) {
                if project_references_media(
                    &persisted,
                    filename,
                    "video/mp4",
                    MAXIMUM_STORY_VIDEO_BYTES,
                ) {
                    return Ok(persisted);
                }
            }
            let _ = fs::remove_file(&asset);
            return Err(error);
        }
        Ok(project)
    }

    pub(super) fn load_project(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<StoryProjectSnapshot, String> {
        let project = self.load_native_project(owner_user_id, project_id)?;
        Ok(StoryProjectSnapshot::from_project(&project))
    }

    pub(super) fn load_native_project(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<StoryProject, String> {
        let owner = validate_owner(owner_user_id)?;
        let project_id = canonical_project_id(project_id)?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(store_error)?;
        load_project_manifest(&projects.join(&project_id), &project_id)
    }

    pub(super) fn load_page(
        &self,
        owner_user_id: &str,
        after: Option<StoryProjectCursor>,
        requested_limit: usize,
    ) -> Result<StoryProjectPage, String> {
        let page = self.load_native_page(owner_user_id, after, requested_limit)?;
        Ok(StoryProjectPage {
            items: page
                .items
                .iter()
                .map(StoryProjectSnapshot::from_project)
                .collect(),
            next_cursor: page.next_cursor,
            unreadable_count: page.unreadable_count,
        })
    }

    pub(super) fn load_native_page(
        &self,
        owner_user_id: &str,
        after: Option<StoryProjectCursor>,
        requested_limit: usize,
    ) -> Result<NativeStoryProjectPage, String> {
        let owner = validate_owner(owner_user_id)?;
        if let Some(cursor) = &after {
            validate_cursor(cursor)?;
        }
        let limit = if requested_limit == 0 {
            DEFAULT_PROJECT_PAGE_SIZE
        } else {
            requested_limit.clamp(1, MAXIMUM_PROJECT_PAGE_SIZE)
        };
        let Some(projects_directory) = self.existing_projects_directory(owner)? else {
            return Ok(NativeStoryProjectPage {
                items: Vec::new(),
                next_cursor: None,
                unreadable_count: 0,
            });
        };

        let mut candidates = Vec::with_capacity(limit.saturating_add(1));
        for entry in fs::read_dir(&projects_directory).map_err(|_| store_error())? {
            let Ok(entry) = entry else { continue };
            let project_id = entry.file_name().to_string_lossy().into_owned();
            if canonical_project_id(&project_id).is_err() {
                continue;
            }
            let project_directory = entry.path();
            let Ok(directory_metadata) = fs::symlink_metadata(&project_directory) else {
                continue;
            };
            if !is_private_directory_metadata(&directory_metadata) {
                continue;
            }
            let manifest = project_directory.join("project.json");
            let Ok(metadata) = fs::symlink_metadata(&manifest) else {
                continue;
            };
            let cursor = StoryProjectCursor {
                modified_at_unix_ms: system_time_milliseconds(metadata.modified().ok()),
                record_id: project_id,
            };
            if after
                .as_ref()
                .is_some_and(|after| !cursor_is_after(&cursor, after))
            {
                continue;
            }
            retain_candidate(
                ProjectCandidate {
                    directory: project_directory,
                    cursor,
                },
                &mut candidates,
                limit.saturating_add(1),
            );
        }

        let has_more = candidates.len() > limit;
        let mut projects = Vec::with_capacity(limit);
        let mut unreadable_count = 0usize;
        for candidate in candidates.iter().take(limit) {
            match load_project_manifest(&candidate.directory, &candidate.cursor.record_id) {
                Ok(project) => projects.push(project),
                Err(_) => unreadable_count = unreadable_count.saturating_add(1),
            }
        }
        projects.sort_by(|left, right| {
            right
                .updated_at_unix_ms
                .cmp(&left.updated_at_unix_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        let next_cursor = has_more
            .then(|| {
                candidates
                    .get(limit.saturating_sub(1))
                    .map(|value| value.cursor.clone())
            })
            .flatten();
        Ok(NativeStoryProjectPage {
            items: projects,
            next_cursor,
            unreadable_count,
        })
    }

    pub(super) fn read_media_asset(
        &self,
        owner_hash: &str,
        project_id: &str,
        filename: &str,
        expected_mime_type: &str,
        maximum_bytes: usize,
    ) -> Result<Vec<u8>, String> {
        if !valid_owner_hash(owner_hash)
            || !valid_storage_filename(filename)
            || !matches!(
                maximum_bytes,
                MAXIMUM_STORY_IMAGE_BYTES | MAXIMUM_STORY_VIDEO_BYTES
            )
        {
            return Err(store_error());
        }
        let project_id = canonical_project_id(project_id)?;
        let projects = self
            .existing_projects_directory_for_hash(owner_hash)?
            .ok_or_else(store_error)?;
        let directory = projects.join(&project_id);
        let project = load_project_manifest(&directory, &project_id)?;
        if !project_references_media(&project, filename, expected_mime_type, maximum_bytes) {
            return Err(store_error());
        }
        read_private_file(&directory.join(filename), maximum_bytes)
    }

    pub(super) fn save_planning_run(
        &self,
        owner_user_id: &str,
        run: &StoryPlanningRun,
    ) -> Result<(), String> {
        let owner = validate_owner(owner_user_id)?;
        let owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(&run.project_id)?;
        let run_id = canonical_project_id(run.id())?;
        run.validate(&owner_hash, &project_id)?;
        let bytes = serde_json::to_vec(run).map_err(|_| planning_store_error())?;
        if bytes.is_empty() || bytes.len() > MAXIMUM_PLANNING_RUN_BYTES {
            return Err(planning_store_error());
        }
        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(planning_store_error)?;
        let project_directory = projects.join(&project_id);
        validate_private_directory(&project_directory)?;
        let runs_directory = private_child_directory(&project_directory, "runs")?;
        let target = runs_directory.join(format!("{run_id}.json"));
        if target.exists() {
            let existing_bytes = read_private_file(&target, MAXIMUM_PLANNING_RUN_BYTES)?;
            let existing: StoryPlanningRun =
                serde_json::from_slice(&existing_bytes).map_err(|_| planning_store_error())?;
            existing.validate(&owner_hash, &project_id)?;
            if (existing.applied && !run.applied)
                || (existing.abandoned_at_unix_ms.is_some() && run.abandoned_at_unix_ms.is_none())
            {
                return Err("剧情规划运行已经结束，不能被迟到结果覆盖。".into());
            }
        }
        commit_private_json(
            &runs_directory,
            &format!("{run_id}.json.tmp"),
            &format!("{run_id}.json"),
            &bytes,
        )
    }

    pub(super) fn load_planning_run(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<StoryPlanningRun, String> {
        let owner = validate_owner(owner_user_id)?;
        let expected_owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(project_id)?;
        let run_id = canonical_project_id(run_id)?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(planning_store_error)?;
        let project_directory = projects.join(&project_id);
        validate_private_directory(&project_directory)?;
        let runs_directory = project_directory.join("runs");
        validate_private_directory(&runs_directory)?;
        let bytes = read_private_file(
            &runs_directory.join(format!("{run_id}.json")),
            MAXIMUM_PLANNING_RUN_BYTES,
        )?;
        let run: StoryPlanningRun =
            serde_json::from_slice(&bytes).map_err(|_| planning_store_error())?;
        if run.id() != run_id {
            return Err(planning_store_error());
        }
        run.validate(&expected_owner_hash, &project_id)?;
        Ok(run)
    }

    pub(super) fn load_planning_history(
        &self,
        owner_user_id: &str,
        project_id: &str,
        after: Option<StoryProjectCursor>,
        requested_limit: usize,
    ) -> Result<StoryPlanningHistoryPage, String> {
        let owner = validate_owner(owner_user_id)?;
        let expected_owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(project_id)?;
        if let Some(cursor) = &after {
            validate_cursor(cursor)?;
        }
        let limit = if requested_limit == 0 {
            DEFAULT_HISTORY_PAGE_SIZE
        } else {
            requested_limit.clamp(1, MAXIMUM_PROJECT_PAGE_SIZE)
        };
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(planning_store_error)?;
        let project_directory = projects.join(&project_id);
        validate_private_directory(&project_directory)?;
        let runs_directory = project_directory.join("runs");
        if !runs_directory.exists() {
            return Ok(StoryPlanningHistoryPage {
                runs: Vec::new(),
                next_cursor: None,
                unreadable_count: 0,
                recovered_project: None,
            });
        }
        validate_private_directory(&runs_directory)?;
        let mut candidates = Vec::with_capacity(limit.saturating_add(1));
        for entry in fs::read_dir(&runs_directory).map_err(|_| planning_store_error())? {
            let Ok(entry) = entry else { continue };
            let filename = entry.file_name().to_string_lossy().into_owned();
            let Some(run_id) = filename.strip_suffix(".json") else {
                continue;
            };
            if canonical_project_id(run_id).is_err() {
                continue;
            }
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                continue;
            }
            let cursor = StoryProjectCursor {
                modified_at_unix_ms: system_time_milliseconds(metadata.modified().ok()),
                record_id: run_id.to_owned(),
            };
            if after
                .as_ref()
                .is_some_and(|after| !cursor_is_after(&cursor, after))
            {
                continue;
            }
            candidates.push((path, cursor));
        }
        candidates.sort_by(|left, right| {
            right
                .1
                .modified_at_unix_ms
                .cmp(&left.1.modified_at_unix_ms)
                .then_with(|| right.1.record_id.cmp(&left.1.record_id))
        });
        candidates.truncate(limit.saturating_add(1));
        let has_more = candidates.len() > limit;
        let next_cursor = has_more
            .then(|| {
                candidates
                    .get(limit.saturating_sub(1))
                    .map(|item| item.1.clone())
            })
            .flatten();
        let mut runs = Vec::with_capacity(limit);
        let mut unreadable_count = 0usize;
        for (path, cursor) in candidates.iter().take(limit) {
            let loaded = read_private_file(path, MAXIMUM_PLANNING_RUN_BYTES)
                .and_then(|bytes| {
                    serde_json::from_slice::<StoryPlanningRun>(&bytes)
                        .map_err(|_| planning_store_error())
                })
                .and_then(|run| {
                    if run.id() != cursor.record_id {
                        return Err(planning_store_error());
                    }
                    run.validate(&expected_owner_hash, &project_id)?;
                    Ok(run)
                });
            match loaded {
                Ok(run) => runs.push(run),
                Err(_) => unreadable_count = unreadable_count.saturating_add(1),
            }
        }
        runs.sort_by(|left, right| {
            right
                .updated_at_unix_ms
                .cmp(&left.updated_at_unix_ms)
                .then_with(|| right.id().cmp(left.id()))
        });

        let mut recovered_project = None;
        if after.is_none() {
            let current = load_project_manifest(&project_directory, &project_id)?;
            if let Some(index) = runs.iter().position(|run| {
                !run.applied
                    && run.abandoned_at_unix_ms.is_none()
                    && run.checkpoint.pending_calls.is_empty()
                    && run.checkpoint.in_flight_call_id.is_none()
                    && super::tools::validate_completion(run).is_ok()
                    && (project_fingerprint(&current).ok().as_ref()
                        == Some(&run.base_project_fingerprint)
                        || project_fingerprint(&current).ok()
                            == project_fingerprint(&run.draft).ok())
            }) {
                let mut candidate = runs[index].clone();
                candidate.checkpoint.status = StoryPlanningStatus::Completed;
                candidate
                    .checkpoint
                    .completion_result
                    .get_or_insert_with(|| "本阶段规划完成，已通过客户端恢复校验。".into());
                candidate.checkpoint.result = candidate.checkpoint.completion_result.clone();
                if let Ok((applied, project)) = self.apply_planning_run(
                    owner,
                    &candidate,
                    system_time_milliseconds(Some(std::time::SystemTime::now())),
                ) {
                    runs[index] = applied;
                    recovered_project = Some(StoryProjectSnapshot::from_project(&project));
                }
            }
        }
        Ok(StoryPlanningHistoryPage {
            runs: runs.iter().map(|run| run.snapshot(owner)).collect(),
            next_cursor,
            unreadable_count,
            recovered_project,
        })
    }

    pub(super) fn save_media_batch(
        &self,
        owner_user_id: &str,
        batch: &StoryMediaBatchRecord,
    ) -> Result<(), String> {
        let owner = validate_owner(owner_user_id)?;
        let expected_owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(&batch.project_id)?;
        let batch_id = canonical_project_id(&batch.id)?;
        batch.validate(&expected_owner_hash, &project_id)?;
        let bytes = serde_json::to_vec(batch).map_err(|_| media_batch_store_error())?;
        if bytes.is_empty() || bytes.len() > MAXIMUM_MEDIA_BATCH_BYTES {
            return Err(media_batch_store_error());
        }
        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(media_batch_store_error)?;
        let project_directory = projects.join(&project_id);
        validate_private_directory(&project_directory)?;
        commit_private_json(
            &project_directory,
            &format!("media-batch-{batch_id}.json.tmp"),
            &format!("media-batch-{batch_id}.json"),
            &bytes,
        )
    }

    pub(super) fn load_media_batch(
        &self,
        owner_user_id: &str,
        project_id: &str,
        batch_id: &str,
    ) -> Result<StoryMediaBatchRecord, String> {
        let owner = validate_owner(owner_user_id)?;
        let expected_owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(project_id)?;
        let batch_id = canonical_project_id(batch_id)?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(media_batch_store_error)?;
        let project_directory = projects.join(&project_id);
        validate_private_directory(&project_directory)?;
        let path = project_directory.join(format!("media-batch-{batch_id}.json"));
        let bytes = read_private_file(&path, MAXIMUM_MEDIA_BATCH_BYTES)?;
        let batch: StoryMediaBatchRecord =
            serde_json::from_slice(&bytes).map_err(|_| media_batch_store_error())?;
        if batch.id != batch_id {
            return Err(media_batch_store_error());
        }
        batch.validate(&expected_owner_hash, &project_id)?;
        Ok(batch)
    }

    pub(super) fn load_media_batch_history(
        &self,
        owner_user_id: &str,
        project_id: &str,
        after: Option<StoryProjectCursor>,
        requested_limit: usize,
    ) -> Result<StoryMediaBatchHistoryPage, String> {
        let owner = validate_owner(owner_user_id)?;
        let expected_owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(project_id)?;
        if let Some(cursor) = &after {
            validate_cursor(cursor)?;
        }
        let limit = if requested_limit == 0 {
            DEFAULT_HISTORY_PAGE_SIZE
        } else {
            requested_limit.clamp(1, MAXIMUM_PROJECT_PAGE_SIZE)
        };
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(media_batch_store_error)?;
        let project_directory = projects.join(&project_id);
        validate_private_directory(&project_directory)?;
        let mut candidates = Vec::with_capacity(limit.saturating_add(1));
        for entry in fs::read_dir(&project_directory).map_err(|_| media_batch_store_error())? {
            let Ok(entry) = entry else { continue };
            let filename = entry.file_name().to_string_lossy().into_owned();
            let Some(batch_id) = filename
                .strip_prefix("media-batch-")
                .and_then(|value| value.strip_suffix(".json"))
            else {
                continue;
            };
            if canonical_project_id(batch_id).is_err() {
                continue;
            }
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !is_private_file_metadata(&metadata) {
                continue;
            }
            let cursor = StoryProjectCursor {
                modified_at_unix_ms: system_time_milliseconds(metadata.modified().ok()),
                record_id: batch_id.to_owned(),
            };
            if after
                .as_ref()
                .is_some_and(|after| !cursor_is_after(&cursor, after))
            {
                continue;
            }
            candidates.push((path, cursor));
        }
        candidates.sort_by(|left, right| {
            right
                .1
                .modified_at_unix_ms
                .cmp(&left.1.modified_at_unix_ms)
                .then_with(|| right.1.record_id.cmp(&left.1.record_id))
        });
        candidates.truncate(limit.saturating_add(1));
        let has_more = candidates.len() > limit;
        let next_cursor = has_more
            .then(|| {
                candidates
                    .get(limit.saturating_sub(1))
                    .map(|item| item.1.clone())
            })
            .flatten();
        let mut batches = Vec::with_capacity(limit);
        let mut unreadable_count = 0usize;
        for (path, cursor) in candidates.iter().take(limit) {
            let loaded = read_private_file(path, MAXIMUM_MEDIA_BATCH_BYTES)
                .and_then(|bytes| {
                    serde_json::from_slice::<StoryMediaBatchRecord>(&bytes)
                        .map_err(|_| media_batch_store_error())
                })
                .and_then(|batch| {
                    if batch.id != cursor.record_id {
                        return Err(media_batch_store_error());
                    }
                    batch.validate(&expected_owner_hash, &project_id)?;
                    Ok(batch)
                });
            match loaded {
                Ok(batch) => batches.push(batch.snapshot(owner)),
                Err(_) => unreadable_count = unreadable_count.saturating_add(1),
            }
        }
        batches.sort_by(|left, right| {
            right
                .updated_at_unix_ms
                .cmp(&left.updated_at_unix_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(StoryMediaBatchHistoryPage {
            batches,
            next_cursor,
            unreadable_count,
        })
    }

    pub(super) fn inspect_media_batches(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<(bool, usize), String> {
        let owner = validate_owner(owner_user_id)?;
        let expected_owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(project_id)?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(media_batch_store_error)?;
        let project_directory = projects.join(&project_id);
        validate_private_directory(&project_directory)?;
        let mut unfinished = false;
        let mut unreadable = 0usize;
        for entry in fs::read_dir(&project_directory).map_err(|_| media_batch_store_error())? {
            let Ok(entry) = entry else { continue };
            let filename = entry.file_name().to_string_lossy().into_owned();
            let Some(batch_id) = filename
                .strip_prefix("media-batch-")
                .and_then(|value| value.strip_suffix(".json"))
            else {
                continue;
            };
            if canonical_project_id(batch_id).is_err() {
                continue;
            }
            let loaded = read_private_file(&entry.path(), MAXIMUM_MEDIA_BATCH_BYTES)
                .and_then(|bytes| {
                    serde_json::from_slice::<StoryMediaBatchRecord>(&bytes)
                        .map_err(|_| media_batch_store_error())
                })
                .and_then(|batch| {
                    if batch.id != batch_id {
                        return Err(media_batch_store_error());
                    }
                    batch.validate(&expected_owner_hash, &project_id)?;
                    Ok(batch)
                });
            match loaded {
                Ok(batch) => {
                    if !matches!(
                        batch.status,
                        super::batch::StoryMediaBatchStatus::Completed
                            | super::batch::StoryMediaBatchStatus::Abandoned
                    ) {
                        unfinished = true;
                    }
                }
                Err(_) => unreadable = unreadable.saturating_add(1),
            }
        }
        Ok((unfinished, unreadable))
    }

    pub(super) fn apply_planning_run(
        &self,
        owner_user_id: &str,
        run: &StoryPlanningRun,
        now_unix_ms: u64,
    ) -> Result<(StoryPlanningRun, StoryProject), String> {
        let owner = validate_owner(owner_user_id)?;
        let owner_hash = owner_hash(owner);
        let project_id = canonical_project_id(&run.project_id)?;
        run.validate(&owner_hash, &project_id)?;
        if run.applied
            || run.abandoned_at_unix_ms.is_some()
            || run.checkpoint.status != StoryPlanningStatus::Completed
        {
            return Err(planning_store_error());
        }
        let _guard = self.mutation_lock.lock().map_err(|_| store_error())?;
        let projects = self
            .existing_projects_directory(owner)?
            .ok_or_else(planning_store_error)?;
        let project_directory = projects.join(&project_id);
        let current = load_project_manifest(&project_directory, &project_id)?;
        let current_fingerprint = project_fingerprint(&current)?;
        let draft_fingerprint = project_fingerprint(&run.draft)?;
        if current_fingerprint != run.base_project_fingerprint
            && current_fingerprint != draft_fingerprint
        {
            return Err("项目已被修改，不能覆盖旧规划；请保留草稿并启动新的规划。".into());
        }
        let mut project = if current_fingerprint == draft_fingerprint {
            current.clone()
        } else {
            run.draft.clone()
        };
        if current_fingerprint != draft_fingerprint {
            project.updated_at_unix_ms = now_unix_ms.max(current.updated_at_unix_ms);
        }
        project.validate()?;
        if current_fingerprint != draft_fingerprint {
            let bytes = serde_json::to_vec(&project).map_err(|_| store_error())?;
            if bytes.is_empty() || bytes.len() > MAXIMUM_PROJECT_BYTES {
                return Err(store_error());
            }
            commit_project_manifest(&project_directory, &bytes)?;
        }
        let mut applied = run.clone();
        applied.draft = project.clone();
        applied.applied = true;
        applied.updated_at_unix_ms = now_unix_ms;
        let bytes = serde_json::to_vec(&applied).map_err(|_| planning_store_error())?;
        if bytes.is_empty() || bytes.len() > MAXIMUM_PLANNING_RUN_BYTES {
            return Err(planning_store_error());
        }
        let runs_directory = private_child_directory(&project_directory, "runs")?;
        commit_private_json(
            &runs_directory,
            &format!("{}.json.tmp", applied.id()),
            &format!("{}.json", applied.id()),
            &bytes,
        )?;
        Ok((applied, project))
    }

    fn ensure_projects_directory(&self, owner: &str) -> Result<PathBuf, String> {
        validate_private_directory(&self.root)?;
        let accounts = private_child_directory(&self.root, "Accounts")?;
        let account = private_child_directory(&accounts, &owner_hash(owner))?;
        private_child_directory(&account, "Projects")
    }

    fn save_locked(&self, owner: &str, project: &StoryProject) -> Result<(), String> {
        project.validate()?;
        let project_id = canonical_project_id(&project.id)?;
        if project.id != project_id {
            return Err(store_error());
        }
        let bytes = serde_json::to_vec(project).map_err(|_| store_error())?;
        if bytes.is_empty() || bytes.len() > MAXIMUM_PROJECT_BYTES {
            return Err(store_error());
        }
        let projects = self.ensure_projects_directory(owner)?;
        let project_directory = private_child_directory(&projects, &project_id)?;
        commit_project_manifest(&project_directory, &bytes)
    }

    fn existing_projects_directory(&self, owner: &str) -> Result<Option<PathBuf>, String> {
        self.existing_projects_directory_for_hash(&owner_hash(owner))
    }

    fn existing_projects_directory_for_hash(
        &self,
        owner_hash: &str,
    ) -> Result<Option<PathBuf>, String> {
        if !valid_owner_hash(owner_hash) {
            return Err(store_error());
        }
        validate_private_directory(&self.root)?;
        let accounts = self.root.join("Accounts");
        if !validate_optional_private_directory(&accounts)? {
            return Ok(None);
        }
        let account = accounts.join(owner_hash);
        if !validate_optional_private_directory(&account)? {
            return Ok(None);
        }
        let projects = account.join("Projects");
        if !validate_optional_private_directory(&projects)? {
            return Ok(None);
        }
        Ok(Some(projects))
    }
}

fn project_references_media(
    project: &StoryProject,
    filename: &str,
    expected_mime_type: &str,
    maximum_bytes: usize,
) -> bool {
    if maximum_bytes == MAXIMUM_STORY_IMAGE_BYTES {
        return project
            .characters
            .iter()
            .flat_map(|value| &value.media.images)
            .chain(project.scenes.iter().flat_map(|value| &value.media.images))
            .chain(project.props.iter().flat_map(|value| &value.media.images))
            .chain(
                project
                    .segments
                    .iter()
                    .flat_map(|value| value.first_frames.images.iter()),
            )
            .chain(
                project
                    .segments
                    .iter()
                    .flat_map(|value| value.last_frames.images.iter()),
            )
            .any(|image| image.filename == filename && image.mime_type == expected_mime_type);
    }
    expected_mime_type == "video/mp4"
        && project
            .segments
            .iter()
            .flat_map(|segment| segment.video.iter().chain(segment.archived_videos.iter()))
            .any(|video| video.filename == filename)
}

#[derive(Clone)]
struct ProjectCandidate {
    directory: PathBuf,
    cursor: StoryProjectCursor,
}

fn load_project_manifest(
    directory: &Path,
    expected_project_id: &str,
) -> Result<StoryProject, String> {
    validate_private_directory(directory)?;
    let bytes = read_private_file(&directory.join("project.json"), MAXIMUM_PROJECT_BYTES)?;
    let project: StoryProject = serde_json::from_slice(&bytes).map_err(|_| store_error())?;
    if project.id != expected_project_id {
        return Err(store_error());
    }
    project.validate()?;
    Ok(project)
}

fn commit_project_manifest(directory: &Path, bytes: &[u8]) -> Result<(), String> {
    commit_private_json(directory, "project.json.tmp", "project.json", bytes)
}

fn media_batch_store_error() -> String {
    "批量制作记录暂时无法读取或保存。".into()
}

fn commit_private_json(
    directory: &Path,
    temporary_name: &str,
    target_name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    if !valid_storage_filename(temporary_name) || !valid_storage_filename(target_name) {
        return Err(store_error());
    }
    validate_private_directory(directory)?;
    let temporary = directory.join(temporary_name);
    let target = directory.join(target_name);
    if let Ok(metadata) = fs::symlink_metadata(&temporary) {
        if !is_private_file_metadata(&metadata) {
            return Err(store_error());
        }
        fs::remove_file(&temporary).map_err(|_| store_error())?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&target) {
        if !is_private_file_metadata(&metadata) {
            return Err(store_error());
        }
    }
    write_private_file(&temporary, bytes)?;
    if let Err(error) = replace_file(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_directory(directory)
}

fn read_private_file(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| store_error())?;
    if !is_private_file_metadata(&link_metadata)
        || link_metadata.len() == 0
        || link_metadata.len() > maximum_bytes as u64
    {
        return Err(store_error());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path).map_err(|_| store_error())?;
    let before = file.metadata().map_err(|_| store_error())?;
    if !is_private_file_metadata(&before) {
        return Err(store_error());
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take((maximum_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| store_error())?;
    let after = file.metadata().map_err(|_| store_error())?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() != before.len() as usize
        || bytes.len() > maximum_bytes
    {
        return Err(store_error());
    }
    Ok(bytes)
}

fn private_child_directory(parent: &Path, name: &str) -> Result<PathBuf, String> {
    validate_private_directory(parent)?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !is_private_directory_metadata(&metadata) {
                return Err(store_error());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|_| store_error())?;
        }
        Err(_) => return Err(store_error()),
    }
    set_private_directory_permissions(&path)?;
    validate_private_directory(&path)?;
    sync_directory(parent)?;
    Ok(path)
}

fn validate_private_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| store_error())?;
    if !is_private_directory_metadata(&metadata) {
        return Err(store_error());
    }
    Ok(())
}

fn validate_optional_private_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_private_directory_metadata(&metadata) => Ok(true),
        Ok(_) => Err(store_error()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(store_error()),
    }
}

fn is_private_directory_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
}

fn is_private_file_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_windows_reparse_point(_: &fs::Metadata) -> bool {
    false
}

fn set_private_directory_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| store_error())?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| store_error())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| store_error())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|_| store_error())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(store_error())
    } else {
        Ok(())
    }
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| store_error())?;
    }
    Ok(())
}

fn retain_candidate(
    candidate: ProjectCandidate,
    values: &mut Vec<ProjectCandidate>,
    capacity: usize,
) {
    let index = values
        .iter()
        .position(|value| cursor_is_newer(&candidate.cursor, &value.cursor))
        .unwrap_or(values.len());
    if index >= capacity {
        return;
    }
    values.insert(index, candidate);
    if values.len() > capacity {
        values.pop();
    }
}

fn cursor_is_newer(left: &StoryProjectCursor, right: &StoryProjectCursor) -> bool {
    left.modified_at_unix_ms > right.modified_at_unix_ms
        || (left.modified_at_unix_ms == right.modified_at_unix_ms
            && left.record_id > right.record_id)
}

fn cursor_is_after(value: &StoryProjectCursor, cursor: &StoryProjectCursor) -> bool {
    value.modified_at_unix_ms < cursor.modified_at_unix_ms
        || (value.modified_at_unix_ms == cursor.modified_at_unix_ms
            && value.record_id < cursor.record_id)
}

fn validate_cursor(cursor: &StoryProjectCursor) -> Result<(), String> {
    canonical_project_id(&cursor.record_id).map(|_| ())
}

fn canonical_project_id(value: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(value).map_err(|_| store_error())?;
    let canonical = parsed.to_string();
    if canonical != value {
        return Err(store_error());
    }
    Ok(canonical)
}

fn validate_owner(value: &str) -> Result<&str, String> {
    let owner = value.trim();
    if owner.is_empty() || owner.len() > 1_024 {
        return Err("剧情项目缺少有效的账号上下文。".into());
    }
    Ok(owner)
}

fn owner_hash(owner_user_id: &str) -> String {
    format!("{:x}", Sha256::digest(owner_user_id.as_bytes()))
}

fn valid_owner_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn system_time_milliseconds(value: Option<std::time::SystemTime>) -> u64 {
    value
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|value| value.as_millis().try_into().ok())
        .unwrap_or_default()
}

fn store_error() -> String {
    "剧情项目文件或记录无效。".into()
}

fn planning_store_error() -> String {
    "剧情规划运行记录无效，原文件已保留。".into()
}
