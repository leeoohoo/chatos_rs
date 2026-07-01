// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};

use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use tokio::fs;
use tracing::warn;

use super::*;
use crate::models::{CreateSkillRequest, UpdateSkillRequest};

const DEFAULT_SKILL_REGISTRY: &str = "github";
const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_WEB_BASE_URL: &str = "https://github.com";
const GITHUB_RAW_HOST_PREFIX: &str = "https://raw.githubusercontent.com/";
const GITHUB_REPOSITORY_SEARCH_LIMIT: usize = 20;
const GITHUB_SKILL_RESULT_LIMIT: usize = 200;
const GITHUB_SKILLS_PER_REPO_LIMIT: usize = 80;
const DEFAULT_MARKETPLACE_PAGE_LIMIT: usize = 10;
const MAX_MARKETPLACE_PAGE_LIMIT: usize = 50;
const SKILL_CONTENT_MAX_CHARS: usize = 200_000;
const MAX_SKILL_PACKAGE_FILES: usize = 200;
const MAX_SKILL_PACKAGE_TOTAL_BYTES: u64 = 10 * 1024 * 1024;
const MAX_SKILL_PACKAGE_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SKILL_PACKAGE_DIRECTORY_DEPTH: usize = 8;

const SKILL_PACKAGE_SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".venv",
    "venv",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
];

const BUNDLED_SKILL_REGISTRY: &str = "codex-bundled";
const MAX_BUNDLED_SKILL_PACKAGE_FILES: usize = 600;
const MAX_BUNDLED_SKILL_PACKAGE_TOTAL_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepositorySearchResponse {
    #[serde(default)]
    items: Vec<GitHubRepository>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepository {
    full_name: String,
    #[serde(default)]
    description: Option<String>,
    default_branch: String,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    stargazers_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubContentItem {
    name: String,
    path: String,
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    download_url: Option<String>,
}

#[derive(Debug, Clone)]
struct GitHubSkillSource {
    repository: GitHubRepository,
    skill_path: String,
    skill_dir: String,
}

#[derive(Debug, Clone)]
struct SkillPackageInstall {
    package_root: String,
    manifest: Vec<SkillPackageFile>,
    total_files: usize,
    total_bytes: u64,
    source_repo: String,
    source_ref: String,
    source_path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeSkillContext {
    pub(crate) skill: SkillRecord,
    pub(crate) package_runtime_path: Option<String>,
}

impl SkillService {
    pub(crate) fn new(config: &AppConfig, store: AppStore) -> Self {
        Self {
            store,
            package_root: installed_skill_packages_root(&config.default_workspace_dir),
        }
    }

    pub async fn list_skills(&self, filters: SkillListFilters) -> Result<Vec<SkillRecord>, String> {
        let mut skills = self.store.list_skills().await?;
        skills.retain(|skill| skill_matches_filters(skill, &filters));
        Ok(skills)
    }

    pub async fn list_bundled_skills(&self) -> Result<Vec<SkillRecord>, String> {
        let mut skills = self
            .store
            .list_skills()
            .await?
            .into_iter()
            .filter(|skill| {
                skill.source == SkillSource::Bundled && skill.scope == SkillScope::AdminGlobal
            })
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| {
            left.display_name
                .to_ascii_lowercase()
                .cmp(&right.display_name.to_ascii_lowercase())
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(skills)
    }

    pub async fn search_installed_skills_for_user(
        &self,
        keyword: Option<String>,
        limit: Option<usize>,
        current_user: &CurrentUser,
    ) -> Result<Vec<SkillRecord>, String> {
        let keyword = normalized_optional(keyword).map(|value| value.to_ascii_lowercase());
        let limit = limit.unwrap_or(20).clamp(1, 100);
        let mut skills = self
            .store
            .list_skills()
            .await?
            .into_iter()
            .filter(|skill| skill.enabled && skill.install_status == SkillInstallStatus::Installed)
            .filter(|skill| skill_visible_to_user(skill, current_user))
            .filter(|skill| {
                keyword
                    .as_deref()
                    .is_none_or(|keyword| skill_matches_search_keyword(skill, keyword))
            })
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| {
            skill_source_rank(left.source)
                .cmp(&skill_source_rank(right.source))
                .then_with(|| {
                    left.display_name
                        .to_ascii_lowercase()
                        .cmp(&right.display_name.to_ascii_lowercase())
                })
                .then_with(|| left.id.cmp(&right.id))
        });
        skills.truncate(limit);
        Ok(skills)
    }

    pub async fn get_skill_for_user(
        &self,
        id: &str,
        current_user: &CurrentUser,
    ) -> Result<Option<SkillRecord>, String> {
        Ok(self
            .store
            .get_skill(id)
            .await?
            .filter(|skill| skill_visible_to_user(skill, current_user)))
    }

    pub async fn sync_bundled_skills(&self) -> Result<usize, String> {
        let skill_paths = discover_bundled_skill_paths().await;
        let mut synced = 0_usize;
        let mut seen = HashSet::new();
        for skill_path in skill_paths {
            let Some(skill_dir) = skill_path.parent().map(Path::to_path_buf) else {
                continue;
            };
            let content = match fs::read_to_string(&skill_path).await {
                Ok(content) => content.trim().to_string(),
                Err(err) => {
                    warn!(
                        path = skill_path.to_string_lossy().as_ref(),
                        "skip bundled skill after read failure: {err}"
                    );
                    continue;
                }
            };
            if content.is_empty() {
                continue;
            }
            let parsed = parse_skill_document(&content);
            let name = bundled_skill_name_from_path(&skill_path, &content);
            if !seen.insert(name.clone()) {
                continue;
            }
            let record_id = format!("bundled:{name}");
            let package = match install_local_skill_package(
                &self.package_root,
                record_id.as_str(),
                &skill_dir,
                &skill_path,
            )
            .await
            {
                Ok(package) => package,
                Err(err) => {
                    warn!(
                        skill = name.as_str(),
                        path = skill_path.to_string_lossy().as_ref(),
                        "skip bundled skill after package copy failure: {err}"
                    );
                    continue;
                }
            };
            let now = now_rfc3339();
            let existing = self.store.get_skill(&record_id).await?;
            let display_name = parsed
                .title
                .clone()
                .unwrap_or_else(|| title_from_slug(&name));
            let record = SkillRecord {
                id: record_id,
                name: name.clone(),
                display_name,
                description: parsed.description,
                content,
                locale: parsed.locale,
                tags: normalize_strings(vec![
                    "openai".to_string(),
                    "codex".to_string(),
                    "bundled".to_string(),
                    name.clone(),
                ]),
                source: SkillSource::Bundled,
                source_url: None,
                source_registry: Some(BUNDLED_SKILL_REGISTRY.to_string()),
                source_package_id: Some(name),
                version: Some(package.source_ref.clone()),
                checksum: None,
                package_root: Some(package.package_root),
                package_manifest: package.manifest,
                package_file_count: package.total_files,
                package_total_bytes: package.total_bytes,
                source_repo: Some(package.source_repo),
                source_ref: Some(package.source_ref),
                source_path: Some(package.source_path),
                install_status: SkillInstallStatus::Installed,
                enabled: existing.as_ref().map(|skill| skill.enabled).unwrap_or(true),
                auto_inject: existing
                    .as_ref()
                    .map(|skill| skill.auto_inject)
                    .unwrap_or(false),
                scope: SkillScope::AdminGlobal,
                creator_user_id: None,
                creator_username: None,
                creator_display_name: Some("OpenAI".to_string()),
                owner_user_id: None,
                owner_username: None,
                owner_display_name: Some("All users".to_string()),
                created_at: existing
                    .as_ref()
                    .map(|skill| skill.created_at.clone())
                    .unwrap_or_else(|| now.clone()),
                updated_at: now.clone(),
                installed_at: Some(now),
            };
            validate_skill(&record)?;
            self.store.save_skill(record).await?;
            synced += 1;
        }
        Ok(synced)
    }

    pub async fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>, String> {
        self.store.get_skill(id).await
    }

    pub async fn create_skill(
        &self,
        input: CreateSkillRequest,
        creator: &CurrentUser,
    ) -> Result<SkillRecord, String> {
        let now = now_rfc3339();
        let display_name = normalize_required_string("display_name", input.display_name)?;
        let source_url = normalized_optional(input.source_url);
        let content = skill_content_from_input(input.content, source_url.as_deref()).await?;
        let name = normalized_optional(input.name)
            .unwrap_or_else(|| slug_from_display_name(&display_name));
        let record = SkillRecord {
            id: Uuid::new_v4().to_string(),
            name: normalize_required_string("name", name)?,
            display_name,
            description: normalized_optional(input.description),
            content,
            locale: normalize_locale(input.locale),
            tags: normalize_strings(input.tags),
            source: if source_url.is_some() {
                SkillSource::Url
            } else {
                SkillSource::Manual
            },
            source_url,
            source_registry: None,
            source_package_id: None,
            version: None,
            checksum: None,
            package_root: None,
            package_manifest: Vec::new(),
            package_file_count: 0,
            package_total_bytes: 0,
            source_repo: None,
            source_ref: None,
            source_path: None,
            install_status: SkillInstallStatus::Installed,
            enabled: input.enabled.unwrap_or(true),
            auto_inject: input.auto_inject.unwrap_or(false),
            scope: SkillScope::User,
            creator_user_id: Some(creator.id.clone()),
            creator_username: Some(creator.username.clone()),
            creator_display_name: Some(creator.display_name.clone()),
            owner_user_id: creator.effective_owner_user_id().map(ToOwned::to_owned),
            owner_username: creator.effective_owner_username().map(ToOwned::to_owned),
            owner_display_name: creator
                .effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| creator.effective_owner_username().map(ToOwned::to_owned)),
            created_at: now.clone(),
            updated_at: now,
            installed_at: None,
        };
        validate_skill(&record)?;
        self.store.save_skill(record).await
    }

    pub async fn update_skill(
        &self,
        id: &str,
        patch: UpdateSkillRequest,
    ) -> Result<Option<SkillRecord>, String> {
        let Some(mut record) = self.store.get_skill(id).await? else {
            return Ok(None);
        };

        if let Some(name) = patch.name {
            record.name = normalize_required_string("name", name)?;
        }
        if let Some(display_name) = patch.display_name {
            record.display_name = normalize_required_string("display_name", display_name)?;
        }
        if patch.description.is_some() {
            record.description = normalized_optional(patch.description);
        }
        if patch.content.is_some() || patch.source_url.is_some() {
            remove_skill_package_dir(&self.package_root, &record).await;
            let source_url = normalized_optional(patch.source_url);
            let content = skill_content_from_input(patch.content, source_url.as_deref()).await?;
            record.content = content;
            clear_package_install(&mut record);
            if source_url.is_some() {
                record.source = SkillSource::Url;
                record.source_url = source_url;
            }
        }
        if patch.locale.is_some() {
            record.locale = normalize_locale(patch.locale);
        }
        if let Some(tags) = patch.tags {
            record.tags = normalize_strings(tags);
        }
        if let Some(enabled) = patch.enabled {
            record.enabled = enabled;
            record.install_status = if enabled {
                SkillInstallStatus::Installed
            } else {
                SkillInstallStatus::Disabled
            };
        }
        if let Some(auto_inject) = patch.auto_inject {
            record.auto_inject = auto_inject;
        }

        record.updated_at = now_rfc3339();
        validate_skill(&record)?;
        Ok(Some(self.store.save_skill(record).await?))
    }

    pub async fn delete_skill(&self, id: &str) -> Result<bool, String> {
        let existing = self.store.get_skill(id).await?;
        let deleted = self.store.delete_skill(id).await?;
        if deleted {
            if let Some(skill) = existing.as_ref() {
                remove_skill_package_dir(&self.package_root, skill).await;
            }
        }
        Ok(deleted)
    }

    pub async fn search_marketplace(
        &self,
        query: SkillMarketplaceQuery,
    ) -> Result<PaginatedResponse<SkillMarketplaceEntry>, String> {
        let installed = self.store.list_skills().await?;
        search_github_skill_marketplace(query, &installed).await
    }

    pub async fn install_marketplace_skill(
        &self,
        input: InstallSkillRequest,
        creator: &CurrentUser,
    ) -> Result<SkillRecord, String> {
        let registry = input
            .registry
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_SKILL_REGISTRY);
        if registry != DEFAULT_SKILL_REGISTRY {
            return Err(format!("暂不支持该 skill registry: {registry}"));
        }
        let source_url = normalize_github_skill_source_url(&input.package_id)?;
        let client = github_client()?;
        let content = fetch_skill_content(&source_url).await?;
        let parsed = parse_skill_document(&content);
        let source_info = parse_github_skill_source(&source_url)?;
        let source_name = github_skill_name_from_source_url(&source_url);
        let display_name = parsed
            .title
            .clone()
            .unwrap_or_else(|| title_from_slug(&source_name));
        let now = now_rfc3339();
        let mut existing = self.store.list_skills().await?.into_iter().find(|skill| {
            skill.source_registry.as_deref() == Some(DEFAULT_SKILL_REGISTRY)
                && skill.source_package_id.as_deref() == Some(source_url.as_str())
                && skill.owner_user_id.as_deref() == creator.effective_owner_user_id()
        });
        let record_id = existing
            .as_ref()
            .map(|record| record.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let package = install_github_skill_package(
            &client,
            &self.package_root,
            record_id.as_str(),
            &source_info,
        )
        .await?;

        let tags = normalize_strings(vec![
            "github".to_string(),
            "marketplace".to_string(),
            parsed.locale.clone(),
        ]);
        let record = if let Some(mut record) = existing.take() {
            record.name = source_name;
            record.display_name = display_name;
            record.description = parsed.description;
            record.content = content;
            record.locale = parsed.locale;
            record.tags = tags;
            record.source = SkillSource::Registry;
            record.source_url = Some(source_url.clone());
            record.source_registry = Some(DEFAULT_SKILL_REGISTRY.to_string());
            record.source_package_id = Some(source_url.clone());
            apply_package_install(&mut record, package);
            record.install_status = SkillInstallStatus::Installed;
            record.enabled = input.enabled.unwrap_or(true);
            record.auto_inject = input.auto_inject.unwrap_or(record.auto_inject);
            record.updated_at = now.clone();
            record.installed_at = Some(now);
            record
        } else {
            SkillRecord {
                id: record_id,
                name: source_name,
                display_name,
                description: parsed.description,
                content,
                locale: parsed.locale,
                tags,
                source: SkillSource::Registry,
                source_url: Some(source_url.clone()),
                source_registry: Some(DEFAULT_SKILL_REGISTRY.to_string()),
                source_package_id: Some(source_url),
                version: None,
                checksum: None,
                package_root: Some(package.package_root),
                package_file_count: package.total_files,
                package_total_bytes: package.total_bytes,
                source_repo: Some(package.source_repo),
                source_ref: Some(package.source_ref),
                source_path: Some(package.source_path),
                package_manifest: package.manifest,
                install_status: SkillInstallStatus::Installed,
                enabled: input.enabled.unwrap_or(true),
                auto_inject: input.auto_inject.unwrap_or(false),
                scope: SkillScope::User,
                creator_user_id: Some(creator.id.clone()),
                creator_username: Some(creator.username.clone()),
                creator_display_name: Some(creator.display_name.clone()),
                owner_user_id: creator.effective_owner_user_id().map(ToOwned::to_owned),
                owner_username: creator.effective_owner_username().map(ToOwned::to_owned),
                owner_display_name: creator
                    .effective_owner_display_name()
                    .map(ToOwned::to_owned)
                    .or_else(|| creator.effective_owner_username().map(ToOwned::to_owned)),
                created_at: now.clone(),
                updated_at: now.clone(),
                installed_at: Some(now),
            }
        };
        validate_skill(&record)?;
        self.store.save_skill(record).await
    }
}

impl RunService {
    pub(crate) async fn runtime_skills_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<Vec<SkillRecord>, String> {
        let task_owner_user_id = task
            .owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                task.creator_user_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            });
        let selected_skill_ids = task
            .mcp_config
            .skill_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<HashSet<_>>();
        let mut skills = self
            .store
            .list_skills()
            .await?
            .into_iter()
            .filter(|skill| skill.enabled && skill.install_status == SkillInstallStatus::Installed)
            .filter(|skill| skill.auto_inject || selected_skill_ids.contains(skill.id.as_str()))
            .filter(|skill| {
                skill.scope == SkillScope::AdminGlobal
                    || skill.owner_user_id.as_deref() == task_owner_user_id
                    || skill.creator_user_id.as_deref() == task_owner_user_id
            })
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| {
            left.display_name
                .to_ascii_lowercase()
                .cmp(&right.display_name.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(skills)
    }

    pub(crate) async fn runtime_skill_contexts_for_task(
        &self,
        task: &TaskRecord,
        workspace_dir: &str,
    ) -> Result<Vec<RuntimeSkillContext>, String> {
        let skills = self.runtime_skills_for_task(task).await?;
        let package_root = installed_skill_packages_root(&self.config.default_workspace_dir);
        materialize_runtime_skill_packages(skills, workspace_dir, &package_root).await
    }
}

#[derive(Debug, Clone)]
struct ParsedSkillDocument {
    title: Option<String>,
    description: Option<String>,
    locale: String,
}

async fn search_github_skill_marketplace(
    query: SkillMarketplaceQuery,
    installed: &[SkillRecord],
) -> Result<PaginatedResponse<SkillMarketplaceEntry>, String> {
    let keyword = query
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("codex skills");
    let limit = query
        .limit
        .unwrap_or(DEFAULT_MARKETPLACE_PAGE_LIMIT)
        .clamp(1, MAX_MARKETPLACE_PAGE_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let desired_count = offset
        .saturating_add(limit)
        .saturating_add(1)
        .min(GITHUB_SKILL_RESULT_LIMIT);
    let locale = normalized_optional(query.locale).map(|value| value.to_ascii_lowercase());
    let tag = normalized_optional(query.tag).map(|value| value.to_ascii_lowercase());
    let client = github_client()?;
    let repositories = marketplace_github_repositories(&client, keyword).await?;
    let mut entries = Vec::new();
    for repository in repositories {
        if entries.len() >= desired_count {
            break;
        }
        let remaining = desired_count - entries.len();
        let mut repo_entries =
            discover_github_skill_entries(&client, &repository, installed, remaining).await;
        if let Some(locale) = locale.as_ref() {
            repo_entries.retain(|entry| entry.locale.eq_ignore_ascii_case(locale));
        }
        if let Some(tag) = tag.as_ref() {
            repo_entries.retain(|entry| {
                entry
                    .tags
                    .iter()
                    .any(|entry_tag| entry_tag.eq_ignore_ascii_case(tag))
            });
        }
        entries.extend(repo_entries);
    }
    entries = entries_for_keyword(entries, keyword);
    let total_discovered = entries.len();
    let has_more = total_discovered > offset.saturating_add(limit);
    let items = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let total = if has_more {
        offset.saturating_add(items.len()).saturating_add(1)
    } else {
        total_discovered
    };
    Ok(PaginatedResponse {
        items,
        total,
        limit,
        offset,
        has_more,
    })
}

fn github_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("task-runner-skill-marketplace")
        .build()
        .map_err(|err| format!("初始化 GitHub client 失败: {err}"))
}

async fn marketplace_github_repositories(
    client: &reqwest::Client,
    keyword: &str,
) -> Result<Vec<GitHubRepository>, String> {
    let broad_keyword = is_broad_skill_keyword(keyword);
    let mut repositories = if broad_keyword {
        seeded_github_repositories()
    } else {
        Vec::new()
    };
    let mut seen = repositories
        .iter()
        .map(|repository| repository.full_name.clone())
        .collect::<BTreeSet<_>>();
    for query in github_repository_queries(keyword) {
        let Ok(found) = search_github_repositories(client, &query).await else {
            continue;
        };
        for repository in found {
            if seen.insert(repository.full_name.clone()) {
                repositories.push(repository);
            }
        }
        if repositories.len() >= GITHUB_REPOSITORY_SEARCH_LIMIT {
            break;
        }
    }
    if !broad_keyword {
        for repository in seeded_github_repositories() {
            if seen.insert(repository.full_name.clone()) {
                repositories.push(repository);
            }
        }
    }
    Ok(repositories)
}

fn seeded_github_repositories() -> Vec<GitHubRepository> {
    vec![
        github_repository(
            "openai/skills",
            "main",
            "Public OpenAI skills repository",
            &["codex", "skills"],
            None,
        ),
        github_repository(
            "affaan-m/ECC",
            "main",
            "Public Claude/Codex skill collection",
            &["ai-agents", "claude-code", "developer-tools", "skills"],
            None,
        ),
        github_repository(
            "Imbad0202/academic-research-skills-codex",
            "main",
            "Academic research skills for Codex",
            &["academic", "codex", "research", "skills"],
            None,
        ),
    ]
}

fn github_repository(
    full_name: &str,
    default_branch: &str,
    description: &str,
    topics: &[&str],
    stargazers_count: Option<u64>,
) -> GitHubRepository {
    GitHubRepository {
        full_name: full_name.to_string(),
        description: Some(description.to_string()),
        default_branch: default_branch.to_string(),
        topics: topics.iter().map(|topic| topic.to_string()).collect(),
        stargazers_count,
    }
}

fn github_repository_queries(keyword: &str) -> Vec<String> {
    let keyword = keyword.trim();
    if keyword.is_empty() || is_broad_skill_keyword(keyword) {
        return vec![
            "codex skills".to_string(),
            "openai skills".to_string(),
            "agent skills".to_string(),
        ];
    }
    vec![
        format!("{keyword} codex skills"),
        format!("{keyword} agent skills"),
        format!("{keyword} skill"),
    ]
}

fn entries_for_keyword(
    entries: Vec<SkillMarketplaceEntry>,
    keyword: &str,
) -> Vec<SkillMarketplaceEntry> {
    if is_broad_skill_keyword(keyword) {
        return entries;
    }
    let filtered = entries
        .iter()
        .filter(|entry| entry_matches_keyword(entry, keyword))
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        entries
    } else {
        filtered
    }
}

fn entry_matches_keyword(entry: &SkillMarketplaceEntry, keyword: &str) -> bool {
    let words = keyword
        .split_whitespace()
        .map(|word| word.trim().to_ascii_lowercase())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() {
        return true;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        entry.name,
        entry.display_name,
        entry.description,
        entry.tags.join("\n"),
        entry.source_url.as_deref().unwrap_or_default(),
        entry.preview_content.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    words.iter().all(|word| haystack.contains(word))
}

fn is_broad_skill_keyword(keyword: &str) -> bool {
    matches!(
        keyword.trim().to_ascii_lowercase().as_str(),
        "" | "skill" | "skills" | "codex" | "agent" | "agents" | "ai"
    )
}

async fn search_github_repositories(
    client: &reqwest::Client,
    keyword: &str,
) -> Result<Vec<GitHubRepository>, String> {
    let mut search_keyword = keyword.trim().to_string();
    if !search_keyword.to_ascii_lowercase().contains("skill") {
        search_keyword.push_str(" skill");
    }
    let url = format!(
        "{GITHUB_API_BASE_URL}/search/repositories?q={}&sort=stars&order=desc&per_page={GITHUB_REPOSITORY_SEARCH_LIMIT}",
        urlencoding::encode(&search_keyword)
    );
    let response = client
        .get(url)
        .header(ACCEPT, "application/vnd.github+json")
        .header(USER_AGENT, "task-runner-skill-marketplace")
        .send()
        .await
        .map_err(|err| format!("搜索 GitHub skill 仓库失败: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("搜索 GitHub skill 仓库失败: HTTP {status} {body}"));
    }
    let result = response
        .json::<GitHubRepositorySearchResponse>()
        .await
        .map_err(|err| format!("解析 GitHub 仓库搜索结果失败: {err}"))?;
    Ok(result.items)
}

async fn discover_github_skill_entries(
    client: &reqwest::Client,
    repository: &GitHubRepository,
    installed: &[SkillRecord],
    limit: usize,
) -> Vec<SkillMarketplaceEntry> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let root = github_directory_contents(client, repository, "")
        .await
        .unwrap_or_default();

    for item in root.iter().filter(|item| is_skill_file(item)) {
        if entries.len() >= limit {
            break;
        }
        if let Some(entry) = github_skill_entry_from_item(client, repository, item, installed).await
        {
            if seen.insert(entry.package_id.clone()) {
                entries.push(entry);
            }
        }
    }

    for directory in ["skills", ".codex/skills", "codex-skills", "agent-skills"] {
        if entries.len() >= limit {
            break;
        }
        let remaining = limit - entries.len();
        let mut directory_entries = discover_github_skill_entries_in_directory(
            client, repository, directory, installed, remaining,
        )
        .await;
        directory_entries.retain(|entry| seen.insert(entry.package_id.clone()));
        entries.extend(directory_entries);
    }

    entries
}

async fn discover_github_skill_entries_in_directory(
    client: &reqwest::Client,
    repository: &GitHubRepository,
    directory: &str,
    installed: &[SkillRecord],
    limit: usize,
) -> Vec<SkillMarketplaceEntry> {
    let mut entries = Vec::new();
    let mut pending = VecDeque::from([(directory.to_string(), 0_usize)]);
    let mut visited = BTreeSet::new();

    while let Some((current_directory, depth)) = pending.pop_front() {
        if entries.len() >= limit || entries.len() >= GITHUB_SKILLS_PER_REPO_LIMIT {
            break;
        }
        if !visited.insert(current_directory.clone()) {
            continue;
        }
        let Ok(items) = github_directory_contents(client, repository, &current_directory).await
        else {
            continue;
        };

        for item in items {
            if entries.len() >= limit || entries.len() >= GITHUB_SKILLS_PER_REPO_LIMIT {
                break;
            }
            if is_skill_file(&item) {
                if let Some(entry) =
                    github_skill_entry_from_item(client, repository, &item, installed).await
                {
                    entries.push(entry);
                }
                continue;
            }
            if item.item_type != "dir" {
                continue;
            }
            let mut found_in_child_dir = false;
            for file_name in ["SKILL.md", "skill.md"] {
                if entries.len() >= limit || entries.len() >= GITHUB_SKILLS_PER_REPO_LIMIT {
                    break;
                }
                let skill_path = format!("{}/{}", item.path.trim_end_matches('/'), file_name);
                let Ok(Some(skill_item)) =
                    github_file_content_item(client, repository, &skill_path).await
                else {
                    continue;
                };
                if let Some(entry) =
                    github_skill_entry_from_item(client, repository, &skill_item, installed).await
                {
                    entries.push(entry);
                    found_in_child_dir = true;
                    break;
                }
            }
            if !found_in_child_dir && depth < 2 {
                pending.push_back((item.path, depth + 1));
            }
        }
    }

    entries
}

async fn github_directory_contents(
    client: &reqwest::Client,
    repository: &GitHubRepository,
    path: &str,
) -> Result<Vec<GitHubContentItem>, String> {
    let url = github_contents_url(repository, path);
    let response = client
        .get(url)
        .header(ACCEPT, "application/vnd.github+json")
        .header(USER_AGENT, "task-runner-skill-marketplace")
        .send()
        .await
        .map_err(|err| format!("读取 GitHub 仓库目录失败: {err}"))?;
    if response.status().as_u16() == 404 {
        return Ok(Vec::new());
    }
    if !response.status().is_success() {
        return github_directory_contents_from_html(client, repository, path).await;
    }
    let parsed = response.json::<Vec<GitHubContentItem>>().await;
    match parsed {
        Ok(items) => Ok(items),
        Err(_) => github_directory_contents_from_html(client, repository, path).await,
    }
}

async fn github_file_content_item(
    client: &reqwest::Client,
    repository: &GitHubRepository,
    path: &str,
) -> Result<Option<GitHubContentItem>, String> {
    let url = github_contents_url(repository, path);
    let response = client
        .get(url)
        .header(ACCEPT, "application/vnd.github+json")
        .header(USER_AGENT, "task-runner-skill-marketplace")
        .send()
        .await
        .map_err(|err| format!("读取 GitHub skill 文件失败: {err}"))?;
    if response.status().as_u16() == 404 {
        return github_file_content_item_from_raw(client, repository, path).await;
    }
    if !response.status().is_success() {
        return github_file_content_item_from_raw(client, repository, path).await;
    }
    let parsed = response.json::<GitHubContentItem>().await;
    match parsed {
        Ok(item) => Ok(Some(item)),
        Err(_) => github_file_content_item_from_raw(client, repository, path).await,
    }
}

fn github_contents_url(repository: &GitHubRepository, path: &str) -> String {
    let path = path.trim_matches('/');
    let path_part = if path.is_empty() {
        String::new()
    } else {
        format!("/{path}")
    };
    format!(
        "{GITHUB_API_BASE_URL}/repos/{}/contents{}?ref={}",
        repository.full_name,
        path_part,
        urlencoding::encode(&repository.default_branch)
    )
}

async fn github_directory_contents_from_html(
    client: &reqwest::Client,
    repository: &GitHubRepository,
    path: &str,
) -> Result<Vec<GitHubContentItem>, String> {
    let url = github_tree_url(repository, path);
    let html = fetch_url_text(client, &url)
        .await
        .map_err(|err| format!("读取 GitHub HTML 目录失败: {err}"))?;
    Ok(parse_github_directory_html(repository, path, &html))
}

async fn github_file_content_item_from_raw(
    client: &reqwest::Client,
    repository: &GitHubRepository,
    path: &str,
) -> Result<Option<GitHubContentItem>, String> {
    let path = path.trim_matches('/');
    let raw_url = github_raw_url(repository, path);
    if fetch_url_text(client, &raw_url).await.is_err() {
        return Ok(None);
    }
    Ok(Some(GitHubContentItem {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: path.to_string(),
        item_type: "file".to_string(),
        download_url: Some(raw_url),
    }))
}

fn github_tree_url(repository: &GitHubRepository, path: &str) -> String {
    let path = path.trim_matches('/');
    let path_part = if path.is_empty() {
        String::new()
    } else {
        format!("/{path}")
    };
    format!(
        "{GITHUB_WEB_BASE_URL}/{}/tree/{}{}",
        repository.full_name, repository.default_branch, path_part
    )
}

fn github_raw_url(repository: &GitHubRepository, path: &str) -> String {
    format!(
        "{GITHUB_RAW_HOST_PREFIX}{}/{}/{}",
        repository.full_name, repository.default_branch, path
    )
}

fn parse_github_directory_html(
    repository: &GitHubRepository,
    current_path: &str,
    html: &str,
) -> Vec<GitHubContentItem> {
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    let tree_prefix = format!(
        "/{}/tree/{}/",
        repository.full_name, repository.default_branch
    );
    let blob_prefix = format!(
        "/{}/blob/{}/",
        repository.full_name, repository.default_branch
    );
    for (item_type, prefix) in [
        ("dir", tree_prefix.as_str()),
        ("file", blob_prefix.as_str()),
    ] {
        for path in extract_github_html_paths(html, prefix) {
            let Some(path) = direct_child_path(current_path, &path) else {
                continue;
            };
            if !seen.insert(format!("{item_type}:{path}")) {
                continue;
            }
            let name = path.rsplit('/').next().unwrap_or(path.as_str()).to_string();
            let download_url = (item_type == "file").then(|| github_raw_url(repository, &path));
            items.push(GitHubContentItem {
                name,
                path,
                item_type: item_type.to_string(),
                download_url,
            });
        }
    }
    items
}

fn extract_github_html_paths(html: &str, prefix: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = html;
    while let Some(index) = rest.find(prefix) {
        let after_prefix = &rest[index + prefix.len()..];
        let path = after_prefix
            .split(|ch| matches!(ch, '"' | '\'' | '<' | '>' | '?' | '#'))
            .next()
            .unwrap_or_default()
            .trim_matches('/')
            .to_string();
        let consumed = path.len().max(1).min(after_prefix.len());
        if !path.is_empty() {
            paths.push(path);
        }
        if consumed == 0 {
            break;
        }
        rest = &after_prefix[consumed..];
    }
    paths
}

fn direct_child_path(current_path: &str, candidate_path: &str) -> Option<String> {
    let current = current_path.trim_matches('/');
    let candidate = candidate_path.trim_matches('/');
    let remainder = if current.is_empty() {
        candidate
    } else {
        candidate.strip_prefix(&format!("{current}/"))?
    };
    if remainder.is_empty() || remainder.contains('/') {
        return None;
    }
    Some(if current.is_empty() {
        remainder.to_string()
    } else {
        format!("{current}/{remainder}")
    })
}

fn is_skill_file(item: &GitHubContentItem) -> bool {
    item.item_type == "file" && item.name.eq_ignore_ascii_case("SKILL.md")
}

async fn install_github_skill_package(
    client: &reqwest::Client,
    packages_root: &Path,
    skill_id: &str,
    source: &GitHubSkillSource,
) -> Result<SkillPackageInstall, String> {
    fs::create_dir_all(packages_root)
        .await
        .map_err(|err| format!("创建 skill 包目录失败: {err}"))?;
    let package_dir_name = filesystem_safe_dir_name(skill_id, "skill-package");
    let final_dir = packages_root.join(&package_dir_name);
    let staging_dir = packages_root.join(format!("{package_dir_name}.tmp-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging_dir)
        .await
        .map_err(|err| format!("创建 skill 包临时目录失败: {err}"))?;

    let result = install_github_skill_package_to_dir(client, &staging_dir, source).await;
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir).await;
    }
    let mut package = result?;
    if path_exists(&final_dir).await {
        fs::remove_dir_all(&final_dir)
            .await
            .map_err(|err| format!("清理旧 skill 包目录失败: {err}"))?;
    }
    fs::rename(&staging_dir, &final_dir)
        .await
        .map_err(|err| format!("保存 skill 包目录失败: {err}"))?;
    package.package_root = final_dir.to_string_lossy().to_string();
    Ok(package)
}

async fn install_github_skill_package_to_dir(
    client: &reqwest::Client,
    target_dir: &Path,
    source: &GitHubSkillSource,
) -> Result<SkillPackageInstall, String> {
    let files = github_skill_package_files(client, source).await?;
    if files.is_empty() {
        return Err("GitHub skill 包没有可安装的文件".to_string());
    }
    let mut manifest = Vec::new();
    let mut total_bytes = 0_u64;
    for item in files {
        let relative_path = skill_package_relative_path(source.skill_dir.as_str(), &item.path)?;
        let source_url = item
            .download_url
            .clone()
            .unwrap_or_else(|| github_raw_url(&source.repository, &item.path));
        let bytes = match fetch_url_bytes(client, &source_url, MAX_SKILL_PACKAGE_FILE_BYTES).await {
            Ok(bytes) => bytes,
            Err(err) if !relative_path.eq_ignore_ascii_case("SKILL.md") => {
                warn!(
                    path = relative_path.as_str(),
                    source_url = source_url.as_str(),
                    "skip optional skill package file after download failure: {err}"
                );
                continue;
            }
            Err(err) => return Err(err),
        };
        let file_size = bytes.len() as u64;
        total_bytes = total_bytes
            .checked_add(file_size)
            .ok_or_else(|| "skill 包大小溢出".to_string())?;
        if total_bytes > MAX_SKILL_PACKAGE_TOTAL_BYTES {
            return Err(format!(
                "skill 包总大小不能超过 {} MB",
                MAX_SKILL_PACKAGE_TOTAL_BYTES / 1024 / 1024
            ));
        }
        let destination = target_dir.join(relative_path.as_str());
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|err| format!("创建 skill 包文件目录失败: {err}"))?;
        }
        fs::write(&destination, bytes)
            .await
            .map_err(|err| format!("写入 skill 包文件失败: {err}"))?;
        manifest.push(SkillPackageFile {
            path: relative_path,
            size_bytes: file_size,
            source_url: Some(source_url),
        });
    }
    Ok(SkillPackageInstall {
        package_root: String::new(),
        total_files: manifest.len(),
        total_bytes,
        source_repo: source.repository.full_name.clone(),
        source_ref: source.repository.default_branch.clone(),
        source_path: source.skill_path.clone(),
        manifest,
    })
}

async fn github_skill_package_files(
    client: &reqwest::Client,
    source: &GitHubSkillSource,
) -> Result<Vec<GitHubContentItem>, String> {
    let mut files = Vec::new();
    let mut pending = VecDeque::from([(source.skill_dir.clone(), 0_usize)]);
    let mut visited = BTreeSet::new();
    while let Some((directory, depth)) = pending.pop_front() {
        if depth > MAX_SKILL_PACKAGE_DIRECTORY_DEPTH || !visited.insert(directory.clone()) {
            continue;
        }
        let items = github_directory_contents(client, &source.repository, &directory).await?;
        for item in items {
            match item.item_type.as_str() {
                "file" => {
                    if files.len() >= MAX_SKILL_PACKAGE_FILES {
                        return Err(format!(
                            "skill 包文件数不能超过 {MAX_SKILL_PACKAGE_FILES} 个"
                        ));
                    }
                    let _ = skill_package_relative_path(source.skill_dir.as_str(), &item.path)?;
                    files.push(item);
                }
                "dir" => {
                    if depth < MAX_SKILL_PACKAGE_DIRECTORY_DEPTH && !is_skipped_package_dir(&item) {
                        let _ = skill_package_relative_path(source.skill_dir.as_str(), &item.path)?;
                        pending.push_back((item.path, depth + 1));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(files)
}

#[derive(Debug)]
struct LocalSkillPackageFile {
    absolute_path: PathBuf,
    relative_path: String,
    size_bytes: u64,
}

async fn discover_bundled_skill_paths() -> Vec<PathBuf> {
    let mut discovered = Vec::new();
    let mut seen = BTreeSet::new();
    for root in bundled_skill_roots() {
        if !path_exists(&root).await {
            continue;
        }
        for path in discover_skill_paths_under(&root, MAX_SKILL_PACKAGE_DIRECTORY_DEPTH).await {
            let key = path.to_string_lossy().to_string();
            if seen.insert(key) {
                discovered.push(path);
            }
        }
    }
    discovered.sort_by(|left, right| {
        left.to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.to_string_lossy().to_ascii_lowercase())
    });
    discovered
}

fn bundled_skill_roots() -> Vec<PathBuf> {
    let Some(codex_home) = codex_home_dir() else {
        return Vec::new();
    };
    vec![
        codex_home.join("skills").join(".system"),
        codex_home
            .join("plugins")
            .join("cache")
            .join("openai-primary-runtime"),
        codex_home
            .join("plugins")
            .join("cache")
            .join("openai-bundled"),
        codex_home
            .join("plugins")
            .join("cache")
            .join("openai-api-curated"),
    ]
}

fn codex_home_dir() -> Option<PathBuf> {
    std::env::var("CODEX_HOME")
        .ok()
        .map(|value| PathBuf::from(value.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".codex"))
        })
}

async fn discover_skill_paths_under(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut pending = VecDeque::from([(root.to_path_buf(), 0_usize)]);
    while let Some((directory, depth)) = pending.pop_front() {
        let Ok(mut entries) = fs::read_dir(&directory).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if file_type.is_file() {
                if path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
                {
                    paths.push(path);
                }
            } else if file_type.is_dir()
                && depth < max_depth
                && !is_skipped_package_dir_name(&entry.file_name().to_string_lossy())
            {
                pending.push_back((path, depth + 1));
            }
        }
    }
    paths
}

async fn install_local_skill_package(
    packages_root: &Path,
    skill_id: &str,
    skill_dir: &Path,
    skill_path: &Path,
) -> Result<SkillPackageInstall, String> {
    fs::create_dir_all(packages_root)
        .await
        .map_err(|err| format!("创建 bundled skill 包目录失败: {err}"))?;
    let package_dir_name = filesystem_safe_dir_name(skill_id, "skill-package");
    let final_dir = packages_root.join(&package_dir_name);
    let staging_dir = packages_root.join(format!("{package_dir_name}.tmp-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging_dir)
        .await
        .map_err(|err| format!("创建 bundled skill 包临时目录失败: {err}"))?;

    let result = install_local_skill_package_to_dir(&staging_dir, skill_dir, skill_path).await;
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir).await;
    }
    let mut package = result?;
    if path_exists(&final_dir).await {
        fs::remove_dir_all(&final_dir)
            .await
            .map_err(|err| format!("清理旧 bundled skill 包目录失败: {err}"))?;
    }
    fs::rename(&staging_dir, &final_dir)
        .await
        .map_err(|err| format!("保存 bundled skill 包目录失败: {err}"))?;
    package.package_root = final_dir.to_string_lossy().to_string();
    Ok(package)
}

async fn install_local_skill_package_to_dir(
    target_dir: &Path,
    skill_dir: &Path,
    skill_path: &Path,
) -> Result<SkillPackageInstall, String> {
    let files = local_skill_package_files(skill_dir).await?;
    if files.is_empty() {
        return Err("bundled skill 包没有可安装的文件".to_string());
    }
    let (source_repo, source_ref, source_path) = bundled_skill_package_source(skill_path);
    let mut manifest = Vec::new();
    let mut total_bytes = 0_u64;
    for item in files {
        total_bytes = total_bytes
            .checked_add(item.size_bytes)
            .ok_or_else(|| "bundled skill 包大小溢出".to_string())?;
        if total_bytes > MAX_BUNDLED_SKILL_PACKAGE_TOTAL_BYTES {
            return Err(format!(
                "bundled skill 包总大小不能超过 {} MB",
                MAX_BUNDLED_SKILL_PACKAGE_TOTAL_BYTES / 1024 / 1024
            ));
        }
        let destination = target_dir.join(item.relative_path.as_str());
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|err| format!("创建 bundled skill 包文件目录失败: {err}"))?;
        }
        fs::copy(&item.absolute_path, &destination)
            .await
            .map_err(|err| format!("复制 bundled skill 包文件失败: {err}"))?;
        manifest.push(SkillPackageFile {
            path: item.relative_path,
            size_bytes: item.size_bytes,
            source_url: None,
        });
    }
    Ok(SkillPackageInstall {
        package_root: String::new(),
        total_files: manifest.len(),
        total_bytes,
        source_repo,
        source_ref,
        source_path,
        manifest,
    })
}

async fn local_skill_package_files(skill_dir: &Path) -> Result<Vec<LocalSkillPackageFile>, String> {
    let mut files = Vec::new();
    let mut pending = VecDeque::from([(skill_dir.to_path_buf(), 0_usize)]);
    while let Some((directory, depth)) = pending.pop_front() {
        if depth > MAX_SKILL_PACKAGE_DIRECTORY_DEPTH {
            continue;
        }
        let mut entries = fs::read_dir(&directory)
            .await
            .map_err(|err| format!("读取 bundled skill 包目录失败: {err}"))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| format!("读取 bundled skill 包目录项失败: {err}"))?
        {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry
                .file_type()
                .await
                .map_err(|err| format!("读取 bundled skill 包文件类型失败: {err}"))?;
            if file_type.is_dir() {
                if depth < MAX_SKILL_PACKAGE_DIRECTORY_DEPTH
                    && !is_skipped_package_dir_name(&file_name)
                {
                    pending.push_back((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if files.len() >= MAX_BUNDLED_SKILL_PACKAGE_FILES {
                return Err(format!(
                    "bundled skill 包文件数不能超过 {MAX_BUNDLED_SKILL_PACKAGE_FILES} 个"
                ));
            }
            let metadata = fs::metadata(&path)
                .await
                .map_err(|err| format!("读取 bundled skill 包文件信息失败: {err}"))?;
            if metadata.len() > MAX_SKILL_PACKAGE_FILE_BYTES {
                return Err(format!(
                    "bundled skill 包单文件不能超过 {} MB",
                    MAX_SKILL_PACKAGE_FILE_BYTES / 1024 / 1024
                ));
            }
            let relative_path = path
                .strip_prefix(skill_dir)
                .map_err(|err| format!("计算 bundled skill 包相对路径失败: {err}"))?
                .to_string_lossy()
                .replace('\\', "/");
            validate_relative_package_path(&relative_path)?;
            files.push(LocalSkillPackageFile {
                absolute_path: path,
                relative_path,
                size_bytes: metadata.len(),
            });
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn is_skipped_package_dir_name(name: &str) -> bool {
    SKILL_PACKAGE_SKIP_DIRS
        .iter()
        .any(|blocked| name.eq_ignore_ascii_case(blocked))
}

fn bundled_skill_name_from_path(skill_path: &Path, content: &str) -> String {
    let local_name = frontmatter_string_field(content, "name")
        .or_else(|| {
            skill_path
                .parent()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "bundled-skill".to_string());
    let local_name = slug_from_display_name(&local_name);
    if let Some(plugin_name) = plugin_name_from_bundled_skill_path(skill_path) {
        format!("{plugin_name}:{local_name}")
    } else {
        local_name
    }
}

fn plugin_name_from_bundled_skill_path(skill_path: &Path) -> Option<String> {
    let components = path_components(skill_path);
    let cache_index = components
        .windows(2)
        .position(|window| window[0] == "plugins" && window[1] == "cache")?;
    let plugin_name = components.get(cache_index + 3)?;
    Some(slug_from_display_name(plugin_name))
}

fn bundled_skill_package_source(skill_path: &Path) -> (String, String, String) {
    let components = path_components(skill_path);
    if let Some(cache_index) = components
        .windows(2)
        .position(|window| window[0] == "plugins" && window[1] == "cache")
    {
        let provider = components
            .get(cache_index + 2)
            .cloned()
            .unwrap_or_else(|| "codex-plugin-cache".to_string());
        let plugin = components
            .get(cache_index + 3)
            .cloned()
            .unwrap_or_else(|| "unknown-plugin".to_string());
        let version = components
            .get(cache_index + 4)
            .cloned()
            .unwrap_or_else(|| "local".to_string());
        let source_path = components.get(cache_index + 5..).unwrap_or(&[]).join("/");
        return (format!("{provider}/{plugin}"), version, source_path);
    }
    if let Some(codex_index) = components.iter().position(|item| item == ".codex") {
        let source_path = components.get(codex_index + 1..).unwrap_or(&[]).join("/");
        return ("codex-system".to_string(), "local".to_string(), source_path);
    }
    (
        "local-bundled".to_string(),
        "local".to_string(),
        skill_path.to_string_lossy().to_string(),
    )
}

fn path_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect()
}

fn parse_github_skill_source(source_url: &str) -> Result<GitHubSkillSource, String> {
    let Some(rest) = source_url.trim().strip_prefix(GITHUB_RAW_HOST_PREFIX) else {
        return Err("GitHub skill source 必须是 raw.githubusercontent.com 地址".to_string());
    };
    let parts = rest.split('/').collect::<Vec<_>>();
    if parts.len() < 4 {
        return Err("GitHub skill source URL 无效".to_string());
    }
    let owner = parts[0].trim();
    let repo = parts[1].trim();
    let source_ref = parts[2].trim();
    let skill_path = parts[3..].join("/");
    if owner.is_empty() || repo.is_empty() || source_ref.is_empty() || skill_path.is_empty() {
        return Err("GitHub skill source URL 无效".to_string());
    }
    validate_relative_package_path(&skill_path)?;
    if !skill_path.to_ascii_lowercase().ends_with("skill.md") {
        return Err("GitHub marketplace 只支持 SKILL.md 文件".to_string());
    }
    let skill_dir = skill_path
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default();
    Ok(GitHubSkillSource {
        repository: GitHubRepository {
            full_name: format!("{owner}/{repo}"),
            description: None,
            default_branch: source_ref.to_string(),
            topics: Vec::new(),
            stargazers_count: None,
        },
        skill_path,
        skill_dir,
    })
}

fn skill_package_relative_path(skill_dir: &str, item_path: &str) -> Result<String, String> {
    let item_path = item_path.trim_matches('/');
    let skill_dir = skill_dir.trim_matches('/');
    let relative = if skill_dir.is_empty() {
        item_path
    } else if item_path == skill_dir {
        ""
    } else {
        item_path
            .strip_prefix(&format!("{skill_dir}/"))
            .ok_or_else(|| format!("skill 包文件不在 skill 目录内: {item_path}"))?
    };
    validate_relative_package_path(relative)?;
    Ok(relative.to_string())
}

fn validate_relative_package_path(path: &str) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("skill 包文件路径不能为空".to_string());
    }
    let mut has_component = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => has_component = true,
            _ => return Err(format!("skill 包文件路径无效: {path}")),
        }
    }
    if !has_component {
        return Err("skill 包文件路径不能为空".to_string());
    }
    Ok(())
}

fn is_skipped_package_dir(item: &GitHubContentItem) -> bool {
    SKILL_PACKAGE_SKIP_DIRS
        .iter()
        .any(|blocked| item.name.eq_ignore_ascii_case(blocked))
}

async fn fetch_url_bytes(
    client: &reqwest::Client,
    url: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .header(USER_AGENT, "task-runner-skill-package-installer")
        .send()
        .await
        .map_err(|err| format!("下载 skill 包文件失败: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("下载 skill 包文件失败: HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err(format!(
            "skill 包单文件不能超过 {} MB",
            max_bytes / 1024 / 1024
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("读取 skill 包文件失败: {err}"))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "skill 包单文件不能超过 {} MB",
            max_bytes / 1024 / 1024
        ));
    }
    Ok(bytes.to_vec())
}

fn apply_package_install(record: &mut SkillRecord, package: SkillPackageInstall) {
    record.package_root = Some(package.package_root);
    record.package_file_count = package.total_files;
    record.package_total_bytes = package.total_bytes;
    record.source_repo = Some(package.source_repo);
    record.source_ref = Some(package.source_ref);
    record.source_path = Some(package.source_path);
    record.package_manifest = package.manifest;
}

fn clear_package_install(record: &mut SkillRecord) {
    record.package_root = None;
    record.package_manifest.clear();
    record.package_file_count = 0;
    record.package_total_bytes = 0;
    record.source_repo = None;
    record.source_ref = None;
    record.source_path = None;
}

async fn remove_skill_package_dir(packages_root: &Path, skill: &SkillRecord) {
    let Some(package_root) = skill.package_root.as_deref() else {
        return;
    };
    let package_root = package_root.trim();
    if package_root.is_empty() {
        return;
    }
    let package_path = PathBuf::from(package_root);
    if !path_is_inside_directory(packages_root, &package_path) {
        warn!(
            skill_id = skill.id.as_str(),
            package_root, "skip removing skill package outside managed root"
        );
        return;
    }
    if let Err(err) = fs::remove_dir_all(package_root).await {
        warn!(
            skill_id = skill.id.as_str(),
            package_root, "failed to remove skill package directory: {err}"
        );
    }
}

async fn materialize_runtime_skill_packages(
    skills: Vec<SkillRecord>,
    workspace_dir: &str,
    packages_root: &Path,
) -> Result<Vec<RuntimeSkillContext>, String> {
    let workspace = PathBuf::from(workspace_dir);
    let runtime_root = workspace.join(".task-runner").join("skills");
    let mut contexts = Vec::with_capacity(skills.len());
    for skill in skills {
        let package_runtime_path =
            match materialize_runtime_skill_package(&skill, &runtime_root, packages_root).await {
                Ok(path) => path,
                Err(err) => {
                    warn!(
                        skill_id = skill.id.as_str(),
                        skill_name = skill.name.as_str(),
                        "failed to materialize skill package for task run: {err}"
                    );
                    None
                }
            };
        contexts.push(RuntimeSkillContext {
            skill,
            package_runtime_path,
        });
    }
    Ok(contexts)
}

async fn materialize_runtime_skill_package(
    skill: &SkillRecord,
    runtime_root: &Path,
    packages_root: &Path,
) -> Result<Option<String>, String> {
    if skill.package_file_count == 0 {
        return Ok(None);
    }
    let Some(package_root) = skill
        .package_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let source = PathBuf::from(package_root);
    if !path_is_inside_directory(packages_root, &source) {
        return Err(format!("skill 包目录不在受管目录内: {package_root}"));
    }
    if !path_exists(&source).await {
        return Err(format!("skill 包目录不存在: {package_root}"));
    }
    let target_name = runtime_skill_dir_name(skill);
    let target = runtime_root.join(&target_name);
    if path_exists(&target).await {
        fs::remove_dir_all(&target)
            .await
            .map_err(|err| format!("清理运行时 skill 包目录失败: {err}"))?;
    }
    copy_dir_recursive(&source, &target).await?;
    Ok(Some(format!(".task-runner/skills/{target_name}")))
}

async fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .await
        .map_err(|err| format!("创建运行时 skill 包目录失败: {err}"))?;
    let mut pending = VecDeque::from([(source.to_path_buf(), target.to_path_buf())]);
    while let Some((current_source, current_target)) = pending.pop_front() {
        let mut entries = fs::read_dir(&current_source)
            .await
            .map_err(|err| format!("读取 skill 包目录失败: {err}"))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| format!("读取 skill 包目录项失败: {err}"))?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|err| format!("读取 skill 包文件类型失败: {err}"))?;
            let child_source = entry.path();
            let child_target = current_target.join(entry.file_name());
            if file_type.is_dir() {
                fs::create_dir_all(&child_target)
                    .await
                    .map_err(|err| format!("创建运行时 skill 子目录失败: {err}"))?;
                pending.push_back((child_source, child_target));
            } else if file_type.is_file() {
                if let Some(parent) = child_target.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .map_err(|err| format!("创建运行时 skill 文件目录失败: {err}"))?;
                }
                fs::copy(&child_source, &child_target)
                    .await
                    .map_err(|err| format!("复制运行时 skill 文件失败: {err}"))?;
            }
        }
    }
    Ok(())
}

async fn path_exists(path: &Path) -> bool {
    fs::metadata(path).await.is_ok()
}

fn path_is_inside_directory(root: &Path, path: &Path) -> bool {
    absolute_path(path).starts_with(absolute_path(root))
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn runtime_skill_dir_name(skill: &SkillRecord) -> String {
    let slug = skill
        .name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let slug = if slug.is_empty() {
        "skill".to_string()
    } else {
        slug
    };
    filesystem_safe_dir_name(&format!("{}-{}", skill.id, slug), "skill")
}

fn filesystem_safe_dir_name(value: &str, fallback: &str) -> String {
    let raw = value.trim();
    let mut name = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while name.ends_with('.') {
        name.pop();
    }
    if name.is_empty() {
        name = fallback.to_string();
    }

    let mut needs_hash = name != raw;
    if is_windows_reserved_file_name(&name) {
        name = format!("_{name}");
        needs_hash = true;
    }
    if name.chars().count() > 120 {
        name = name.chars().take(120).collect();
        while name.ends_with('.') || name.ends_with('-') {
            name.pop();
        }
        if name.is_empty() {
            name = fallback.to_string();
        }
        needs_hash = true;
    }
    if needs_hash {
        format!("{name}-{:016x}", stable_fnv1a64(raw))
    } else {
        name
    }
}

fn is_windows_reserved_file_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn stable_fnv1a64(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn installed_skill_packages_root(default_workspace_dir: &str) -> PathBuf {
    PathBuf::from(default_workspace_dir)
        .join(".task-runner")
        .join("installed-skills")
}

async fn github_skill_entry_from_item(
    client: &reqwest::Client,
    repository: &GitHubRepository,
    item: &GitHubContentItem,
    installed: &[SkillRecord],
) -> Option<SkillMarketplaceEntry> {
    let raw_url = normalize_github_skill_source_url(item.download_url.as_deref()?).ok()?;
    let content = fetch_url_text(client, &raw_url).await.ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let parsed = parse_skill_document(&content);
    let source_name = github_skill_name_from_path(&item.path);
    let display_name = parsed
        .title
        .clone()
        .unwrap_or_else(|| title_from_slug(&source_name));
    let mut tags = vec![
        "github".to_string(),
        "skill".to_string(),
        parsed.locale.clone(),
    ];
    tags.extend(repository.topics.clone());
    tags.sort();
    tags.dedup();
    let installed_skill = installed.iter().find(|skill| {
        skill.source_registry.as_deref() == Some(DEFAULT_SKILL_REGISTRY)
            && skill.source_package_id.as_deref() == Some(raw_url.as_str())
    });
    Some(SkillMarketplaceEntry {
        registry: DEFAULT_SKILL_REGISTRY.to_string(),
        package_id: raw_url.clone(),
        name: source_name,
        display_name,
        description: parsed
            .description
            .or_else(|| repository.description.clone())
            .unwrap_or_else(|| format!("Public GitHub skill from {}", repository.full_name)),
        locale: parsed.locale,
        tags,
        version: repository
            .stargazers_count
            .map(|count| format!("{count} stars")),
        source_url: Some(raw_url),
        checksum: None,
        package_file_count: 0,
        package_total_bytes: 0,
        installed_skill_id: installed_skill.map(|skill| skill.id.clone()),
        installed: installed_skill.is_some(),
        preview_content: Some(preview_content(&content)),
    })
}

fn normalize_github_skill_source_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.starts_with(GITHUB_RAW_HOST_PREFIX) {
        if !value.to_ascii_lowercase().ends_with("/skill.md") {
            return Err("GitHub marketplace 只支持 SKILL.md 文件".to_string());
        }
        return Ok(value.to_string());
    }
    if let Some(rest) = value.strip_prefix("https://github.com/") {
        let parts = rest.split('/').collect::<Vec<_>>();
        if parts.len() >= 5 && parts[2] == "blob" {
            let owner = parts[0];
            let repo = parts[1];
            let branch = parts[3];
            let path = parts[4..].join("/");
            if path.to_ascii_lowercase().ends_with("skill.md") {
                return Ok(format!(
                    "{GITHUB_RAW_HOST_PREFIX}{owner}/{repo}/{branch}/{path}"
                ));
            }
        }
    }
    Err(
        "GitHub marketplace 只支持 raw.githubusercontent.com 或 github.com/.../blob/.../SKILL.md"
            .to_string(),
    )
}

fn parse_skill_document(content: &str) -> ParsedSkillDocument {
    let mut title = None;
    let mut description = frontmatter_string_field(content, "description");
    for line in content_without_frontmatter(content).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if title.is_none() && line.starts_with('#') {
            let heading = line.trim_start_matches('#').trim();
            if !heading.is_empty() {
                title = Some(heading.to_string());
            }
            continue;
        }
        if description.is_none()
            && !line.starts_with('#')
            && !line.starts_with("```")
            && !line.starts_with("---")
        {
            description = Some(line.trim_matches('*').trim().to_string());
        }
        if title.is_some() && description.is_some() {
            break;
        }
    }
    ParsedSkillDocument {
        title,
        description,
        locale: if contains_cjk(content) {
            "zh-CN".to_string()
        } else {
            "en-US".to_string()
        },
    }
}

fn frontmatter_string_field(content: &str, key: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let prefix = format!("{key}:");
    for line in lines {
        let line = line.trim();
        if line == "---" {
            break;
        }
        let Some(value) = line.strip_prefix(&prefix) else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn content_without_frontmatter(content: &str) -> &str {
    let Some(rest) = content.strip_prefix("---") else {
        return content;
    };
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .unwrap_or(rest);
    let Some(index) = rest.find("\n---") else {
        return content;
    };
    let after = &rest[index + "\n---".len()..];
    after
        .strip_prefix("\r\n")
        .or_else(|| after.strip_prefix('\n'))
        .unwrap_or(after)
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|ch| {
        ('\u{4e00}'..='\u{9fff}').contains(&ch)
            || ('\u{3400}'..='\u{4dbf}').contains(&ch)
            || ('\u{3040}'..='\u{30ff}').contains(&ch)
            || ('\u{ac00}'..='\u{d7af}').contains(&ch)
    })
}

fn github_skill_name_from_source_url(source_url: &str) -> String {
    let path = source_url
        .strip_prefix(GITHUB_RAW_HOST_PREFIX)
        .unwrap_or(source_url);
    github_skill_name_from_path(path)
}

fn github_skill_name_from_path(path: &str) -> String {
    let parts = path
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    let candidate = if parts
        .last()
        .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
    {
        parts
            .get(parts.len().saturating_sub(2))
            .copied()
            .unwrap_or("github-skill")
    } else {
        parts.last().copied().unwrap_or("github-skill")
    };
    slug_from_display_name(candidate)
}

fn title_from_slug(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn skill_content_from_input(
    content: Option<String>,
    source_url: Option<&str>,
) -> Result<String, String> {
    if let Some(content) = content
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(content);
    }
    let Some(source_url) = source_url else {
        return Err("content 不能为空".to_string());
    };
    fetch_skill_content(source_url).await
}

async fn fetch_skill_content(source_url: &str) -> Result<String, String> {
    let url = source_url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("source_url 仅支持 http:// 或 https://".to_string());
    }
    let client = github_client()?;
    fetch_url_text(&client, url).await
}

async fn fetch_url_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header(USER_AGENT, "task-runner-skill-marketplace")
        .send()
        .await
        .map_err(|err| format!("读取 URL 失败: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("读取 URL 失败: HTTP {}", response.status()));
    }
    let content = response
        .text()
        .await
        .map_err(|err| format!("读取 URL 内容失败: {err}"))?;
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("URL 返回内容为空".to_string());
    }
    Ok(content)
}

fn validate_skill(skill: &SkillRecord) -> Result<(), String> {
    validate_required("name", &skill.name)?;
    validate_required("display_name", &skill.display_name)?;
    validate_required("content", &skill.content)?;
    if skill.content.chars().count() > SKILL_CONTENT_MAX_CHARS {
        return Err(format!("content 不能超过 {SKILL_CONTENT_MAX_CHARS} 个字符"));
    }
    if skill.name.len() > 120 {
        return Err("name 不能超过 120 个字符".to_string());
    }
    if skill.display_name.len() > 160 {
        return Err("display_name 不能超过 160 个字符".to_string());
    }
    Ok(())
}

fn skill_matches_filters(skill: &SkillRecord, filters: &SkillListFilters) -> bool {
    if filters
        .enabled
        .is_some_and(|enabled| skill.enabled != enabled)
    {
        return false;
    }
    if filters
        .auto_inject
        .is_some_and(|auto_inject| skill.auto_inject != auto_inject)
    {
        return false;
    }
    if filters.source.is_some_and(|source| skill.source != source) {
        return false;
    }
    if let Some(locale) = filters
        .locale
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !skill.locale.eq_ignore_ascii_case(locale) {
            return false;
        }
    }
    if let Some(keyword) = filters
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
    {
        let haystack = format!(
            "{}\n{}\n{}\n{}",
            skill.name,
            skill.display_name,
            skill.description.as_deref().unwrap_or_default(),
            skill.tags.join("\n")
        )
        .to_ascii_lowercase();
        if !haystack.contains(&keyword) {
            return false;
        }
    }
    true
}

fn skill_visible_to_user(skill: &SkillRecord, current_user: &CurrentUser) -> bool {
    if skill.scope == SkillScope::AdminGlobal {
        return true;
    }
    let owner_user_id = skill
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| skill.creator_user_id.as_deref());
    current_user.can_access_owned_resource(owner_user_id)
}

fn skill_matches_search_keyword(skill: &SkillRecord, keyword: &str) -> bool {
    let words = keyword
        .split_whitespace()
        .map(|word| word.trim().to_ascii_lowercase())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() {
        return true;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        skill.id,
        skill.name,
        skill.display_name,
        skill.description.as_deref().unwrap_or_default(),
        skill.tags.join("\n")
    )
    .to_ascii_lowercase();
    if words.iter().all(|word| haystack.contains(word)) {
        return true;
    }
    let content = skill.content.to_ascii_lowercase();
    words.iter().all(|word| content.contains(word))
}

fn skill_source_rank(source: SkillSource) -> usize {
    match source {
        SkillSource::Bundled => 0,
        SkillSource::Registry => 1,
        SkillSource::Manual => 2,
        SkillSource::Url => 3,
    }
}

fn normalize_required_string(label: &str, value: String) -> Result<String, String> {
    validate_required(label, &value)?;
    Ok(value.trim().to_string())
}

fn normalize_locale(value: Option<String>) -> String {
    normalized_optional(value).unwrap_or_else(|| BuiltinMcpPromptLocale::DEFAULT_KEY.to_string())
}

fn slug_from_display_name(display_name: &str) -> String {
    let slug = display_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "skill".to_string()
    } else {
        slug
    }
}

fn preview_content(content: &str) -> String {
    let content = content.trim();
    const MAX_PREVIEW_CHARS: usize = 8_000;
    if content.chars().count() <= MAX_PREVIEW_CHARS {
        return content.to_string();
    }
    let preview = content.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
    format!("{preview}\n\n...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_safe_dir_name_preserves_safe_ids() {
        let id = "550e8400-e29b-41d4-a716-446655440000";

        assert_eq!(filesystem_safe_dir_name(id, "skill-package"), id);
    }

    #[test]
    fn filesystem_safe_dir_name_rewrites_windows_invalid_chars() {
        let name = filesystem_safe_dir_name("bundled:figma:figma-use", "skill-package");

        assert!(name.starts_with("bundled-figma-figma-use-"));
        assert!(!name
            .chars()
            .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')));
    }

    #[test]
    fn filesystem_safe_dir_name_rewrites_windows_reserved_names() {
        let name = filesystem_safe_dir_name("con", "skill-package");

        assert!(name.starts_with("_con-"));
    }
}
