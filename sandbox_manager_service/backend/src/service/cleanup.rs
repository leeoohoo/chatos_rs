use tokio::time::{sleep, Duration};

use super::SandboxManager;

impl SandboxManager {
    pub fn spawn_cleanup_worker(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let interval = self.config().cleanup_interval.max(Duration::from_secs(5));
            loop {
                sleep(interval).await;
                if let Err(err) = self.cleanup_expired().await {
                    tracing::warn!("sandbox cleanup failed: {}", err);
                }
            }
        })
    }
}
