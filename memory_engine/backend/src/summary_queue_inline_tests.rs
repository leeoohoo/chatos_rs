#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{summary_consumer_enabled, wait_until_consumer_enabled};
    use crate::pressure::{MemoryEnginePressurePolicy, PlatformPressureLevel};

    fn policy(active_summary_concurrency: usize) -> MemoryEnginePressurePolicy {
        MemoryEnginePressurePolicy {
            level: PlatformPressureLevel::Elevated,
            active_summary_concurrency,
            reconcile_paused: false,
            refresh_interval: Duration::from_secs(5),
            queue_elevated_messages: 100,
            queue_critical_messages: 1_000,
        }
    }

    #[test]
    fn pressure_policy_enables_only_the_target_number_of_consumers() {
        let value = policy(2);
        assert!(summary_consumer_enabled(&value, 0));
        assert!(summary_consumer_enabled(&value, 1));
        assert!(!summary_consumer_enabled(&value, 2));
    }

    #[tokio::test]
    async fn paused_consumer_resumes_from_pressure_change_without_polling() {
        let (sender, mut receiver) = tokio::sync::watch::channel(policy(1));
        let waiter = tokio::spawn(async move {
            wait_until_consumer_enabled(&mut receiver, 2).await
        });
        assert!(tokio::time::timeout(Duration::from_millis(20), async {
            while !waiter.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_err());
        sender.send_replace(policy(4));
        assert!(waiter.await.expect("consumer waiter task").is_ok());
    }
}
