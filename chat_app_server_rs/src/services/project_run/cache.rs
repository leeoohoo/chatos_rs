use crate::models::project_run::ProjectRunCatalog;
use crate::models::project_run_environment::{
    ProjectRunEnvironmentSelection, ProjectRunEnvironmentSnapshot,
};
use crate::services::project_local_cache::{read_cache_json, remove_cache_file, write_cache_json};

const RUN_CATALOG_CACHE_PATH: &str = "project_run/catalog.json";
const RUN_ENVIRONMENT_SELECTION_CACHE_PATH: &str = "project_run/environment_selection.json";
const RUN_ENVIRONMENT_SNAPSHOT_CACHE_PATH: &str = "project_run/environment_snapshot.json";

pub(crate) fn read_cached_catalog(project_root: &str) -> Result<Option<ProjectRunCatalog>, String> {
    read_cache_json(project_root, RUN_CATALOG_CACHE_PATH)
}

pub(crate) fn write_cached_catalog(
    project_root: &str,
    catalog: &ProjectRunCatalog,
) -> Result<(), String> {
    write_cache_json(project_root, RUN_CATALOG_CACHE_PATH, catalog)
}

pub(crate) fn read_cached_environment_selection(
    project_root: &str,
) -> Result<Option<ProjectRunEnvironmentSelection>, String> {
    read_cache_json(project_root, RUN_ENVIRONMENT_SELECTION_CACHE_PATH)
}

pub(crate) fn write_cached_environment_selection(
    project_root: &str,
    selection: &ProjectRunEnvironmentSelection,
) -> Result<(), String> {
    write_cache_json(
        project_root,
        RUN_ENVIRONMENT_SELECTION_CACHE_PATH,
        selection,
    )
}

pub(crate) fn read_cached_environment_snapshot(
    project_root: &str,
) -> Result<Option<ProjectRunEnvironmentSnapshot>, String> {
    read_cache_json(project_root, RUN_ENVIRONMENT_SNAPSHOT_CACHE_PATH)
}

pub(crate) fn write_cached_environment_snapshot(
    project_root: &str,
    snapshot: &ProjectRunEnvironmentSnapshot,
) -> Result<(), String> {
    write_cache_json(project_root, RUN_ENVIRONMENT_SNAPSHOT_CACHE_PATH, snapshot)
}

pub(crate) fn clear_cached_environment_snapshot(project_root: &str) -> Result<(), String> {
    remove_cache_file(project_root, RUN_ENVIRONMENT_SNAPSHOT_CACHE_PATH)
}
