// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::Result;
use memory_engine_sdk::MemoryPolicyKind;

use crate::local_runtime::managed_memory_policy;
use crate::{tracing_stdout, LocalRuntime};

use super::service::run_review_inner;

pub(crate) async fn maybe_spawn_local_memory_review(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    session_id: &str,
) -> Result<bool> {
    let Some(settings) = runtime
        .local_database()?
        .get_runtime_settings(owner_user_id, session_id)
        .await?
    else {
        return Ok(false);
    };
    let (pending_messages, pending_characters) = runtime
        .local_database()?
        .pending_memory_stats(owner_user_id, session_id)
        .await?;
    let policy = managed_memory_policy(runtime, MemoryPolicyKind::Summary).await;
    if !should_start_automatic_summary(
        settings.memory_auto_summary_enabled && policy.enabled,
        pending_messages,
        pending_characters,
        settings.memory_summary_message_threshold,
        settings.memory_summary_character_threshold,
    ) {
        return Ok(false);
    }
    let job = match runtime.memory_jobs.register(session_id) {
        Ok(job) => job,
        Err(_) => return Ok(false),
    };
    let runtime = runtime.clone();
    let owner_user_id = owner_user_id.to_string();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let _job = job;
        if let Err(error) = run_review_inner(
            &runtime,
            owner_user_id.as_str(),
            session_id.as_str(),
            "automatic_threshold",
        )
        .await
        {
            tracing_stdout(
                format!("automatic local memory review failed for session {session_id}: {error}")
                    .as_str(),
            );
        }
    });
    Ok(true)
}

fn should_start_automatic_summary(
    enabled: bool,
    pending_messages: i64,
    pending_characters: i64,
    message_threshold: i64,
    character_threshold: i64,
) -> bool {
    enabled
        && (pending_messages >= message_threshold.max(1)
            || pending_characters >= character_threshold.max(1))
}

#[cfg(test)]
mod tests {
    use super::should_start_automatic_summary;

    #[test]
    fn automatic_summary_policy_respects_enablement_and_either_threshold() {
        assert!(!should_start_automatic_summary(
            false, 100, 100_000, 24, 32_000
        ));
        assert!(should_start_automatic_summary(true, 24, 100, 24, 32_000));
        assert!(should_start_automatic_summary(true, 2, 32_000, 24, 32_000));
        assert!(!should_start_automatic_summary(true, 2, 2_000, 24, 32_000));
    }
}
