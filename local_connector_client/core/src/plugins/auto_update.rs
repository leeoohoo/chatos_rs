// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::PluginInstallSource;
use chrono::{DateTime, Duration, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::LocalPluginStatusSnapshot;

const AUTO_UPDATE_SCHEMA_VERSION: u32 = 1;
const MAX_AUTO_UPDATE_STATE_BYTES: u64 = 1024 * 1024;
const INITIAL_RETRY_MINUTES: i64 = 15;
const MAX_RETRY_MINUTES: i64 = 24 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginAutoUpdateRecord {
    pub plugin_id: String,
    #[serde(default)]
    pub target_release_id: Option<String>,
    #[serde(default)]
    pub last_checked_at: Option<String>,
    #[serde(default)]
    pub last_attempted_at: Option<String>,
    #[serde(default)]
    pub last_succeeded_at: Option<String>,
    #[serde(default)]
    pub next_retry_at: Option<String>,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginAutoUpdateState {
    pub schema_version: u32,
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginAutoUpdateRecord>,
}

impl Default for PluginAutoUpdateState {
    fn default() -> Self {
        Self {
            schema_version: AUTO_UPDATE_SCHEMA_VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PluginAutoUpdateDecision {
    Ineligible(&'static str),
    UpToDate,
    Busy,
    Deferred,
    Ready,
}

impl PluginAutoUpdateState {
    pub fn load(plugin_root: &Path) -> Result<Self> {
        let path = auto_update_state_path(plugin_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let metadata = fs::metadata(&path).with_context(|| {
            format!("read Plugin auto-update state metadata: {}", path.display())
        })?;
        if !metadata.is_file() || metadata.len() > MAX_AUTO_UPDATE_STATE_BYTES {
            bail!("Plugin auto-update state is invalid or exceeds the size limit");
        }
        let state = serde_json::from_slice::<Self>(
            fs::read(&path)
                .with_context(|| format!("read Plugin auto-update state: {}", path.display()))?
                .as_slice(),
        )
        .context("parse Plugin auto-update state")?;
        state.validate()?;
        Ok(state)
    }

    pub fn save(&self, plugin_root: &Path) -> Result<()> {
        self.validate()?;
        fs::create_dir_all(plugin_root).with_context(|| {
            format!(
                "create local Plugin storage directory: {}",
                plugin_root.display()
            )
        })?;
        let payload =
            serde_json::to_vec_pretty(self).context("serialize Plugin auto-update state")?;
        if payload.len() as u64 > MAX_AUTO_UPDATE_STATE_BYTES {
            bail!("Plugin auto-update state exceeds the size limit");
        }
        let mut temporary = NamedTempFile::new_in(plugin_root)
            .context("create temporary Plugin auto-update state")?;
        temporary
            .write_all(payload.as_slice())
            .context("write temporary Plugin auto-update state")?;
        temporary
            .as_file()
            .sync_all()
            .context("sync temporary Plugin auto-update state")?;
        temporary
            .persist(auto_update_state_path(plugin_root))
            .map_err(|error| error.error)
            .context("atomically replace Plugin auto-update state")?;
        sync_directory(plugin_root)?;
        Ok(())
    }

    pub(crate) fn mark_checked(&mut self, plugin_id: &str, release_id: &str, now: DateTime<Utc>) {
        let record = self.record_mut(plugin_id);
        if record.target_release_id.as_deref() != Some(release_id) {
            record.target_release_id = Some(release_id.to_string());
            record.next_retry_at = None;
            record.consecutive_failures = 0;
            record.last_error = None;
        }
        record.last_checked_at = Some(now.to_rfc3339());
    }

    pub(crate) fn mark_success(&mut self, plugin_id: &str, release_id: &str, now: DateTime<Utc>) {
        let record = self.record_mut(plugin_id);
        record.target_release_id = Some(release_id.to_string());
        record.last_checked_at = Some(now.to_rfc3339());
        record.last_attempted_at = Some(now.to_rfc3339());
        record.last_succeeded_at = Some(now.to_rfc3339());
        record.next_retry_at = None;
        record.consecutive_failures = 0;
        record.last_error = None;
    }

    pub(crate) fn mark_up_to_date(
        &mut self,
        plugin_id: &str,
        release_id: &str,
        now: DateTime<Utc>,
    ) {
        let record = self.record_mut(plugin_id);
        record.target_release_id = Some(release_id.to_string());
        record.last_checked_at = Some(now.to_rfc3339());
        record.next_retry_at = None;
        record.consecutive_failures = 0;
        record.last_error = None;
    }

    pub(crate) fn mark_failure(
        &mut self,
        plugin_id: &str,
        release_id: &str,
        now: DateTime<Utc>,
        error: &str,
    ) {
        let record = self.record_mut(plugin_id);
        if record.target_release_id.as_deref() != Some(release_id) {
            record.target_release_id = Some(release_id.to_string());
            record.consecutive_failures = 0;
        }
        record.consecutive_failures = record.consecutive_failures.saturating_add(1);
        let exponent = record.consecutive_failures.saturating_sub(1).min(16);
        let multiplier = 1_i64.checked_shl(exponent).unwrap_or(i64::MAX);
        let delay_minutes = INITIAL_RETRY_MINUTES
            .saturating_mul(multiplier)
            .min(MAX_RETRY_MINUTES);
        record.last_checked_at = Some(now.to_rfc3339());
        record.last_attempted_at = Some(now.to_rfc3339());
        record.next_retry_at = Some((now + Duration::minutes(delay_minutes)).to_rfc3339());
        record.last_error = Some(sanitize_error(error));
    }

    pub(crate) fn clear_retry(&mut self, plugin_id: &str) {
        if let Some(record) = self.plugins.get_mut(plugin_id) {
            record.next_retry_at = None;
            record.consecutive_failures = 0;
            record.last_error = None;
        }
    }

    fn record_mut(&mut self, plugin_id: &str) -> &mut PluginAutoUpdateRecord {
        self.plugins
            .entry(plugin_id.to_string())
            .or_insert_with(|| PluginAutoUpdateRecord {
                plugin_id: plugin_id.to_string(),
                target_release_id: None,
                last_checked_at: None,
                last_attempted_at: None,
                last_succeeded_at: None,
                next_retry_at: None,
                consecutive_failures: 0,
                last_error: None,
            })
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != AUTO_UPDATE_SCHEMA_VERSION {
            bail!("unsupported Plugin auto-update state schema version");
        }
        for (plugin_id, record) in &self.plugins {
            if plugin_id != &record.plugin_id
                || plugin_id.trim().is_empty()
                || plugin_id.len() > 256
            {
                bail!("Plugin auto-update state contains an invalid Plugin identity");
            }
            for timestamp in [
                record.last_checked_at.as_deref(),
                record.last_attempted_at.as_deref(),
                record.last_succeeded_at.as_deref(),
                record.next_retry_at.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                DateTime::parse_from_rfc3339(timestamp)
                    .context("Plugin auto-update state contains an invalid timestamp")?;
            }
            if record
                .last_error
                .as_ref()
                .is_some_and(|value| value.len() > 1024)
            {
                bail!("Plugin auto-update state error exceeds the size limit");
            }
        }
        Ok(())
    }
}

pub(crate) fn evaluate_auto_update(
    source: &PluginInstallSource,
    status: &LocalPluginStatusSnapshot,
    record: Option<&PluginAutoUpdateRecord>,
    now: DateTime<Utc>,
) -> PluginAutoUpdateDecision {
    let Some(preference) = source.preference.as_ref() else {
        return PluginAutoUpdateDecision::Ineligible("preference_missing");
    };
    if preference.plugin_id != source.catalog.id
        || !preference.enabled
        || !preference.auto_update
        || preference.release_channel != "stable"
        || source.release.release_channel != "stable"
    {
        return PluginAutoUpdateDecision::Ineligible("policy_disabled");
    }
    let Some(installed) = status.registry.plugins.get(source.catalog.id.as_str()) else {
        return PluginAutoUpdateDecision::Ineligible("not_installed");
    };
    if installed.marketplace_id != source.marketplace.id {
        return PluginAutoUpdateDecision::Ineligible("marketplace_mismatch");
    }
    if status
        .transactions
        .active
        .values()
        .any(|transaction| transaction.plugin_id == source.catalog.id)
    {
        return PluginAutoUpdateDecision::Busy;
    }
    let Some(active_version) = installed.active_version.as_deref() else {
        return PluginAutoUpdateDecision::Ineligible("no_active_version");
    };
    let Some((active, latest)) = Version::parse(active_version)
        .ok()
        .zip(Version::parse(source.release.version.as_str()).ok())
    else {
        return PluginAutoUpdateDecision::Ineligible("invalid_version");
    };
    if latest <= active {
        return PluginAutoUpdateDecision::UpToDate;
    }
    if record.is_some_and(|record| {
        record.target_release_id.as_deref() == Some(source.release.id.as_str())
            && record.next_retry_at.as_deref().is_some_and(|value| {
                DateTime::parse_from_rfc3339(value)
                    .map(|retry| retry.with_timezone(&Utc) > now)
                    .unwrap_or(false)
            })
    }) {
        return PluginAutoUpdateDecision::Deferred;
    }
    PluginAutoUpdateDecision::Ready
}

fn auto_update_state_path(plugin_root: &Path) -> PathBuf {
    plugin_root.join("auto-updates.json")
}

fn sanitize_error(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut sanitized = String::new();
    for character in normalized.chars() {
        if sanitized.len().saturating_add(character.len_utf8()) > 1024 {
            break;
        }
        sanitized.push(character);
    }
    sanitized
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("open Plugin storage directory: {}", path.display()))?
        .sync_all()
        .with_context(|| format!("sync Plugin storage directory: {}", path.display()))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::tests::fixtures::{ArchiveMutation, TestSigner};
    use crate::plugins::PluginInstaller;
    use chatos_plugin_management_sdk::UserPluginPreferenceRecord;

    #[test]
    fn failures_back_off_exponentially_and_new_release_resets_retry() {
        let now = DateTime::parse_from_rfc3339("2026-07-26T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let mut state = PluginAutoUpdateState::default();
        state.mark_failure(
            "plugin-demo",
            "release-1",
            now,
            "first failure\nwith details",
        );
        let first = state.plugins.get("plugin-demo").expect("first failure");
        assert_eq!(first.consecutive_failures, 1);
        assert_eq!(
            first.next_retry_at.as_deref(),
            Some("2026-07-26T00:15:00+00:00")
        );
        assert_eq!(
            first.last_error.as_deref(),
            Some("first failure with details")
        );

        state.mark_failure("plugin-demo", "release-1", now, "second failure");
        let second = state.plugins.get("plugin-demo").expect("second failure");
        assert_eq!(second.consecutive_failures, 2);
        assert_eq!(
            second.next_retry_at.as_deref(),
            Some("2026-07-26T00:30:00+00:00")
        );

        state.mark_checked("plugin-demo", "release-2", now);
        let reset = state.plugins.get("plugin-demo").expect("reset release");
        assert_eq!(reset.consecutive_failures, 0);
        assert_eq!(reset.next_retry_at, None);
        assert_eq!(reset.last_error, None);
    }

    #[test]
    fn state_round_trip_is_atomic_and_validated() {
        let temp = tempfile::TempDir::new().expect("temporary directory");
        let now = Utc::now();
        let mut state = PluginAutoUpdateState::default();
        state.mark_success("plugin-demo", "release-1", now);
        state.save(temp.path()).expect("save state");
        assert_eq!(
            PluginAutoUpdateState::load(temp.path()).expect("load state"),
            state
        );
    }

    #[test]
    fn policy_requires_installed_enabled_stable_plugin_and_honors_retry() {
        let temp = tempfile::TempDir::new().expect("temporary directory");
        let signer = TestSigner::new();
        let installed_package = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
        let installer = PluginInstaller::new(temp.path().join("plugin-store"));
        installer
            .install_archive(installed_package.install_request())
            .expect("install current Plugin");
        let update_package = signer.package(temp.path(), "1.1.0", ArchiveMutation::None);
        let mut source = update_package.install_source();
        source.preference = Some(UserPluginPreferenceRecord {
            owner_user_id: "owner".to_string(),
            plugin_id: source.catalog.id.clone(),
            enabled: true,
            auto_update: true,
            release_channel: "stable".to_string(),
            enabled_components: Vec::new(),
            updated_at: "2026-07-26T00:00:00Z".to_string(),
        });
        let status = installer.status_snapshot().expect("Plugin status");
        let now = DateTime::parse_from_rfc3339("2026-07-26T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        assert_eq!(
            evaluate_auto_update(&source, &status, None, now),
            PluginAutoUpdateDecision::Ready
        );

        let mut state = PluginAutoUpdateState::default();
        state.mark_failure(
            source.catalog.id.as_str(),
            source.release.id.as_str(),
            now,
            "download failed",
        );
        assert_eq!(
            evaluate_auto_update(
                &source,
                &status,
                state.plugins.get(source.catalog.id.as_str()),
                now,
            ),
            PluginAutoUpdateDecision::Deferred
        );

        source.preference.as_mut().expect("preference").auto_update = false;
        assert_eq!(
            evaluate_auto_update(&source, &status, None, now),
            PluginAutoUpdateDecision::Ineligible("policy_disabled")
        );
    }
}
