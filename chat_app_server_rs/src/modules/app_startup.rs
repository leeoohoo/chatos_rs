use tracing::{info, warn};

use crate::services::terminal_manager::get_terminal_manager;
use crate::{config::Config, core, db, services};

pub async fn initialize_runtime(cfg: &Config) -> Result<(), String> {
    if let Err(err) =
        core::remote_connection_error_codes::export_remote_connection_error_code_catalog_doc()
    {
        warn!("Failed to export remote connection error code catalog: {err}");
        core::runtime_health::mark_runtime_check_warn(
            "remote_connection_error_codes",
            false,
            format!("catalog export failed: {err}"),
        );
    } else {
        core::runtime_health::mark_runtime_check_ok(
            "remote_connection_error_codes",
            false,
            "catalog exported",
        );
    }

    db::init_global()
        .await
        .map_err(|err| format!("Failed to init database: {err}"))?;
    core::runtime_health::mark_runtime_check_ok("database", true, "database initialized");

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
                report.source_id,
                report.source_type,
                report.status,
                report.sdk_enabled
            );
            core::runtime_health::mark_runtime_check_ok(
                "memory_engine_source_bootstrap",
                true,
                format!(
                    "source_id={} source_type={} status={} sdk_enabled={}",
                    report.source_id,
                    report.source_type,
                    report.status,
                    report.sdk_enabled
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

    match crate::repositories::ai_model_configs::backfill_ai_model_config_secret_storage().await {
        Ok(report) => {
            info!(
                "AI model config secret backfill finished: total_count={} migrated_count={} skipped_encrypted_count={} empty_count={}",
                report.total_count,
                report.migrated_count,
                report.skipped_encrypted_count,
                report.empty_count
            );
            core::runtime_health::mark_runtime_check_ok(
                "ai_model_config_secret_backfill",
                false,
                format!(
                    "total_count={} migrated_count={} skipped_encrypted_count={} empty_count={}",
                    report.total_count,
                    report.migrated_count,
                    report.skipped_encrypted_count,
                    report.empty_count
                ),
            );
        }
        Err(err) => {
            warn!("AI model config secret backfill failed: {err}");
            core::runtime_health::mark_runtime_check_warn(
                "ai_model_config_secret_backfill",
                false,
                format!("backfill failed: {err}"),
            );
        }
    }

    {
        let manager = get_terminal_manager();
        match manager.cleanup_stale_project_run_terminals().await {
            Ok(count) => {
                if count > 0 {
                    info!("Cleaned stale project-run terminals on startup: {}", count);
                }
                core::runtime_health::mark_runtime_check_ok(
                    "terminal_cleanup",
                    false,
                    format!("cleaned_count={count}"),
                );
            }
            Err(err) => {
                warn!("Failed to cleanup stale project-run terminals on startup: {err}");
                core::runtime_health::mark_runtime_check_warn(
                    "terminal_cleanup",
                    false,
                    format!("cleanup failed: {err}"),
                );
            }
        }
    }

    services::workspace_realtime_watcher::start_workspace_realtime_watcher();
    core::runtime_health::mark_runtime_check_ok(
        "workspace_realtime_watcher",
        true,
        "watcher started",
    );

    info!("Memory-only mode enabled, skip local session background jobs");

    cfg.print();
    Ok(())
}
