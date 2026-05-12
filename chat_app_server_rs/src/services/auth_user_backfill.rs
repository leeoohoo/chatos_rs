use tracing::{info, warn};

use crate::repositories::auth_users;

use super::legacy_auth_store;

#[derive(Debug, Default, Clone)]
pub struct AuthUserBackfillReport {
    pub legacy_count: usize,
    pub created_count: usize,
    pub skipped_existing_count: usize,
    pub skipped_invalid_count: usize,
}

pub async fn backfill_legacy_auth_users() -> Result<AuthUserBackfillReport, String> {
    let legacy_items = legacy_auth_store::list_users().await?;
    if legacy_items.is_empty() {
        info!("legacy auth_users empty, skip auth-user backfill");
        return Ok(AuthUserBackfillReport::default());
    }

    let mut report = AuthUserBackfillReport::default();
    for item in legacy_items {
        report.legacy_count += 1;

        let user_id = item.user_id.trim();
        let password_hash = item.password_hash.trim();
        let role = item.role.trim();
        if user_id.is_empty() || password_hash.is_empty() || role.is_empty() {
            report.skipped_invalid_count += 1;
            warn!(
                "skip invalid legacy auth_user during backfill: user_id={} role={}",
                item.user_id, item.role
            );
            continue;
        }

        if auth_users::get_user_by_id(user_id).await?.is_some() {
            report.skipped_existing_count += 1;
            continue;
        }

        auth_users::upsert_user(&item.into_auth_user_record()).await?;
        report.created_count += 1;
    }

    info!(
        "legacy auth-user backfill completed: legacy_count={} created_count={} skipped_existing_count={} skipped_invalid_count={}",
        report.legacy_count,
        report.created_count,
        report.skipped_existing_count,
        report.skipped_invalid_count
    );

    Ok(report)
}
