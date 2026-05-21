use tracing::{info, warn};

use crate::services::terminal_manager::get_terminal_manager;
use crate::{config::Config, core, db, services};

pub async fn initialize_runtime(cfg: &Config) -> Result<(), String> {
    if let Err(err) =
        core::remote_connection_error_codes::export_remote_connection_error_code_catalog_doc()
    {
        warn!("Failed to export remote connection error code catalog: {err}");
    }

    db::init_global()
        .await
        .map_err(|err| format!("Failed to init database: {err}"))?;

    match services::auth_user_backfill::backfill_legacy_auth_users().await {
        Ok(report) => {
            info!(
                "Legacy auth-user backfill finished: legacy_count={} created_count={} skipped_existing_count={} skipped_invalid_count={}",
                report.legacy_count,
                report.created_count,
                report.skipped_existing_count,
                report.skipped_invalid_count
            );
        }
        Err(err) => {
            warn!("Legacy auth-user backfill failed: {err}");
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
        }
        Err(err) => {
            warn!("Chatos memory_engine source bootstrap failed: {err}");
        }
    }

    {
        let manager = get_terminal_manager();
        match manager.cleanup_stale_project_run_terminals().await {
            Ok(count) => {
                if count > 0 {
                    info!("Cleaned stale project-run terminals on startup: {}", count);
                }
            }
            Err(err) => {
                warn!("Failed to cleanup stale project-run terminals on startup: {err}");
            }
        }
    }

    services::workspace_realtime_watcher::start_workspace_realtime_watcher();

    info!("Memory-only mode enabled, skip local session background jobs");

    cfg.print();
    Ok(())
}
