// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::{info, warn};

use crate::{config::Config, core, db, services};

pub async fn initialize_runtime(cfg: &Config) -> Result<(), String> {
    if let Err(err) =
        core::remote_connection_error_codes::remote_connection_error_code_catalog_json()
    {
        warn!("Failed to render remote connection error code catalog: {err}");
        core::runtime_health::mark_runtime_check_warn(
            "remote_connection_error_codes",
            false,
            format!("catalog render failed: {err}"),
        );
    } else {
        core::runtime_health::mark_runtime_check_ok(
            "remote_connection_error_codes",
            false,
            "catalog rendered",
        );
    }

    db::init_global()
        .await
        .map_err(|err| format!("Failed to init database: {err}"))?;
    core::runtime_health::mark_runtime_check_ok("database", true, "database initialized");

    super::cloud_agent_runtime::initialize().await?;
    core::runtime_health::mark_runtime_check_ok(
        "cloud_agent_store",
        true,
        "Cloud Agent store initialized",
    );
    super::conversation_runtime::cloud_agent::spawn_outbox_reconciler()?;
    super::conversation_runtime::cloud_agent::spawn_consumer()?;
    core::runtime_health::mark_runtime_check_ok(
        "cloud_agent_runtime",
        true,
        "Cloud Agent consumer and outbox reconciler started",
    );

    match crate::repositories::user_settings::purge_managed_runtime_settings().await {
        Ok(modified_count) => {
            info!(
                "Removed legacy managed runtime fields from user preferences: modified_count={modified_count}"
            );
            core::runtime_health::mark_runtime_check_ok(
                "user_preferences_migration",
                false,
                format!("modified_count={modified_count}"),
            );
        }
        Err(err) => {
            warn!("Failed to clean legacy user runtime settings: {err}");
            core::runtime_health::mark_runtime_check_warn(
                "user_preferences_migration",
                false,
                format!("cleanup failed: {err}"),
            );
        }
    }

    match services::auth_user_backfill::backfill_legacy_auth_users().await {
        Ok(report) => {
            info!(
                "Legacy auth-user backfill finished: legacy_count={} created_count={} skipped_existing_count={} skipped_invalid_count={}",
                report.legacy_count,
                report.created_count,
                report.skipped_existing_count,
                report.skipped_invalid_count
            );
            core::runtime_health::mark_runtime_check_ok(
                "auth_user_backfill",
                false,
                format!(
                    "legacy_count={} created_count={} skipped_existing_count={} skipped_invalid_count={}",
                    report.legacy_count,
                    report.created_count,
                    report.skipped_existing_count,
                    report.skipped_invalid_count
                ),
            );
        }
        Err(err) => {
            warn!("Legacy auth-user backfill failed: {err}");
            core::runtime_health::mark_runtime_check_warn(
                "auth_user_backfill",
                false,
                format!("backfill failed: {err}"),
            );
        }
    }

    match services::memory_engine_source_bootstrap::ensure_chatos_memory_engine_source().await {
        Ok(report) => {
            info!(
                "Chatos memory_engine source ensured: source_id={} source_type={} status={} sdk_enabled={}",
                report.source_id, report.source_type, report.status, report.sdk_enabled
            );
            core::runtime_health::mark_runtime_check_ok(
                "memory_engine_source_bootstrap",
                true,
                format!(
                    "source_id={} source_type={} status={} sdk_enabled={}",
                    report.source_id, report.source_type, report.status, report.sdk_enabled
                ),
            );
        }
        Err(err) => {
            warn!("Chatos memory_engine source bootstrap failed: {err}");
            core::runtime_health::mark_runtime_check_warn(
                "memory_engine_source_bootstrap",
                true,
                format!("bootstrap failed: {err}"),
            );
        }
    }

    services::workspace_realtime_watcher::start_workspace_realtime_watcher();
    core::runtime_health::mark_runtime_check_ok(
        "workspace_realtime_watcher",
        true,
        "watcher started",
    );

    services::requirement_execution_reconciler::start_requirement_execution_reconciler();
    core::runtime_health::mark_runtime_check_ok(
        "requirement_execution_reconciler",
        true,
        "reconciler started",
    );

    info!("Memory-only mode enabled, skip local session background jobs");

    cfg.print();
    Ok(())
}
