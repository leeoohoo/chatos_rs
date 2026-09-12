// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::HashMap, sync::Arc};

use chatos_local_agent_runtime::LocalAgentProfile;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileRegistryError {
    #[error("local Agent profile key must not be empty")]
    EmptyKey,
    #[error("local Agent profile key must not contain surrounding whitespace")]
    InvalidKey,
    #[error("local Agent profile {0} is registered more than once")]
    Duplicate(String),
    #[error("local Agent profile {0} is not registered")]
    NotFound(String),
}

#[derive(Clone, Default)]
pub struct LocalAgentProfileRegistry {
    profiles: HashMap<String, Arc<dyn LocalAgentProfile>>,
}

impl LocalAgentProfileRegistry {
    pub fn new(
        profiles: impl IntoIterator<Item = Arc<dyn LocalAgentProfile>>,
    ) -> Result<Self, ProfileRegistryError> {
        let mut registry = Self::default();
        for profile in profiles {
            registry.register(profile)?;
        }
        Ok(registry)
    }

    pub fn register(
        &mut self,
        profile: Arc<dyn LocalAgentProfile>,
    ) -> Result<(), ProfileRegistryError> {
        let raw_key = profile.profile_key();
        let key = raw_key.trim();
        if key.is_empty() {
            return Err(ProfileRegistryError::EmptyKey);
        }
        if key != raw_key {
            return Err(ProfileRegistryError::InvalidKey);
        }
        if self.profiles.contains_key(key) {
            return Err(ProfileRegistryError::Duplicate(key.to_string()));
        }
        self.profiles.insert(key.to_string(), profile);
        Ok(())
    }

    pub fn require(
        &self,
        profile_key: &str,
    ) -> Result<Arc<dyn LocalAgentProfile>, ProfileRegistryError> {
        self.profiles
            .get(profile_key.trim())
            .cloned()
            .ok_or_else(|| ProfileRegistryError::NotFound(profile_key.to_string()))
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}
