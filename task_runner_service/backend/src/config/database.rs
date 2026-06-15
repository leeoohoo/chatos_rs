use super::*;

pub(super) fn default_database_url(store_mode: StoreMode, mongodb_database: &str) -> String {
    match store_mode {
        StoreMode::Memory => "memory://task_runner_service".to_string(),
        StoreMode::Sqlite => "sqlite://task_runner_service/data/task_runner.db".to_string(),
        StoreMode::Mongo => {
            format!("mongodb://admin:admin@127.0.0.1:27018/{mongodb_database}?authSource=admin")
        }
    }
}

pub(super) fn normalize_database_url(
    store_mode: StoreMode,
    database_url: String,
    mongodb_database: &str,
) -> String {
    if store_mode != StoreMode::Mongo {
        return database_url;
    }
    normalize_mongodb_database_url(database_url, mongodb_database)
}

pub(super) fn normalize_mongodb_database_url(
    database_url: String,
    mongodb_database: &str,
) -> String {
    let trimmed = database_url.trim();
    if trimmed.is_empty() {
        return format!(
            "mongodb://admin:admin@127.0.0.1:27018/{mongodb_database}?authSource=admin"
        );
    }

    let (base, query_suffix) = if let Some((base, query)) = trimmed.split_once('?') {
        (base, format!("?{query}"))
    } else {
        (trimmed, String::new())
    };

    let Some(scheme_sep) = base.find("://") else {
        return trimmed.to_string();
    };
    let remainder = &base[(scheme_sep + 3)..];
    match remainder.find('/') {
        None => format!("{base}/{mongodb_database}{query_suffix}"),
        Some(path_idx) => {
            let path = &remainder[(path_idx + 1)..];
            if path.is_empty() {
                let prefix = &base[..(scheme_sep + 3 + path_idx + 1)];
                format!("{prefix}{mongodb_database}{query_suffix}")
            } else {
                trimmed.to_string()
            }
        }
    }
}
