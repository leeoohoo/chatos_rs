pub(crate) fn resolve_lock_lease_seconds() -> i64 {
    std::env::var("MEMORY_SERVER_JOB_LOCK_LEASE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1800)
        .max(60)
}
