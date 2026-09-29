// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Business-profile boundary for the shared durable Local Agent loop.

use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalAgentRunClaim, LocalAgentStepOutcome};
use std::{collections::HashMap, sync::Arc};

#[async_trait]
pub trait LocalAgentProfile: Send + Sync {
    /// Executes exactly one bounded step. Claiming, persistence, retries and
    /// state transitions remain owned by the shared runtime and Host.
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String>;
}

#[derive(Clone, Default)]
pub struct LocalAgentProfileRegistry {
    profiles: HashMap<String, Arc<dyn LocalAgentProfile>>,
}

impl LocalAgentProfileRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<P>(&mut self, profile_key: impl Into<String>, profile: P) -> Result<(), String>
    where
        P: LocalAgentProfile + 'static,
    {
        self.register_shared(profile_key, Arc::new(profile))
    }

    pub fn register_shared(
        &mut self,
        profile_key: impl Into<String>,
        profile: Arc<dyn LocalAgentProfile>,
    ) -> Result<(), String> {
        let profile_key = profile_key.into();
        let profile_key = profile_key.trim();
        if profile_key.is_empty() || profile_key.len() > 256 {
            return Err("Local Agent profile key must be 1..=256 characters".to_string());
        }
        if self.profiles.contains_key(profile_key) {
            return Err(format!(
                "Local Agent profile key is registered twice: {profile_key}"
            ));
        }
        self.profiles.insert(profile_key.to_string(), profile);
        Ok(())
    }

    pub fn profile_for(&self, profile_key: &str) -> Option<Arc<dyn LocalAgentProfile>> {
        self.profiles.get(profile_key).cloned()
    }

    pub fn contains(&self, profile_key: &str) -> bool {
        self.profiles.contains_key(profile_key)
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopProfile;

    #[async_trait]
    impl LocalAgentProfile for NoopProfile {
        async fn execute_step(
            &self,
            _claim: &LocalAgentRunClaim,
        ) -> Result<LocalAgentStepOutcome, String> {
            Ok(LocalAgentStepOutcome::Pause {
                reason: "test".to_string(),
            })
        }
    }

    #[test]
    fn registry_rejects_empty_and_duplicate_keys() {
        let mut registry = LocalAgentProfileRegistry::new();
        assert!(registry.register("", NoopProfile).is_err());
        registry
            .register("main_chat", NoopProfile)
            .expect("register profile");
        assert!(registry.contains("main_chat"));
        assert!(registry.register("main_chat", NoopProfile).is_err());
    }
}
