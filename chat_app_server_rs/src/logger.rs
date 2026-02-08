use crate::config::Config;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use once_cell::sync::OnceCell;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use tracing_appender::rolling;
use tracing_subscriber::fmt::time::UtcTime;

struct LoggerGuards {
    _file: tracing_appender::non_blocking::WorkerGuard,
    _error: tracing_appender::non_blocking::WorkerGuard,
}

static LOG_GUARDS: OnceCell<LoggerGuards> = OnceCell::new();

pub fn init_logger(cfg: &Config) -> Result<(), String> {
    let log_dir = Path::new("logs");
    if !log_dir.exists() {
        fs::create_dir_all(log_dir).map_err(|e| format!("create log dir failed: {e}"))?;
    }

    cleanup_old_logs(log_dir, &cfg.log_max_files);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(cfg.log_level.clone()));

    let file_appender = rolling::daily(log_dir, "server.log");
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

    let error_appender = rolling::daily(log_dir, "error.log");
    let (error_writer, error_guard) = tracing_appender::non_blocking(error_appender);

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_timer(UtcTime::rfc_3339())
        .with_thread_ids(true)
        .with_writer(std::io::stdout);

    let file_layer = fmt::layer()
        .with_target(false)
        .with_timer(UtcTime::rfc_3339())
        .with_thread_ids(true)
        .json()
        .with_writer(file_writer);

    let error_layer = fmt::layer()
        .with_target(false)
        .with_timer(UtcTime::rfc_3339())
        .with_thread_ids(true)
        .json()
        .with_writer(error_writer)
        .with_filter(tracing_subscriber::filter::LevelFilter::ERROR);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(file_layer)
        .with(error_layer)
        .init();

    let _ = LOG_GUARDS.set(LoggerGuards { _file: file_guard, _error: error_guard });

    std::panic::set_hook(Box::new(|panic_info| {
        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "panic occurred".to_string()
        };
        let location = panic_info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_else(|| "unknown".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(panic = %payload, location = %location, backtrace = %backtrace, "panic");
    }));

    Ok(())
}

fn cleanup_old_logs(log_dir: &Path, max_files: &str) {
    let keep_days = parse_keep_days(max_files);
    if keep_days == 0 {
        return;
    }
    let cutoff = SystemTime::now().checked_sub(Duration::from_secs(keep_days * 24 * 3600));
    let Ok(entries) = fs::read_dir(log_dir) else { return; };
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if let Some(cut) = cutoff {
                    if modified < cut {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

fn parse_keep_days(value: &str) -> u64 {
    let raw = value.trim().to_lowercase();
    if raw.ends_with('d') {
        return raw.trim_end_matches('d').parse::<u64>().unwrap_or(0);
    }
    raw.parse::<u64>().unwrap_or(0)
}
