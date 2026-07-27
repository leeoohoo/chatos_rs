// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::{SecureRandom, SystemRandom};

const HANDLE_PREFIX: &str = "psh_";
const HANDLE_RANDOM_BYTES: usize = 24;
const MAX_HANDLE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub(super) struct PluginSecretHandleRegistry {
    handles: Arc<Mutex<HashMap<String, HandleRecord>>>,
}

#[derive(Debug, Clone)]
struct HandleRecord {
    scope_hash: String,
    expires_at: Instant,
}

impl Default for PluginSecretHandleRegistry {
    fn default() -> Self {
        Self {
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl PluginSecretHandleRegistry {
    pub(super) fn issue(&self, scope_hash: &str, ttl: Duration) -> Result<String> {
        if ttl.is_zero() || ttl > MAX_HANDLE_TTL {
            bail!("Plugin secret handle TTL must be between 1 millisecond and 15 minutes");
        }
        let mut random = [0_u8; HANDLE_RANDOM_BYTES];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| anyhow::anyhow!("generate Plugin secret handle failed"))?;
        let handle = format!("{HANDLE_PREFIX}{}", URL_SAFE_NO_PAD.encode(random));
        let mut handles = self
            .handles
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin secret handle lock is poisoned"))?;
        handles.retain(|_, record| record.expires_at > Instant::now());
        handles.insert(
            handle.clone(),
            HandleRecord {
                scope_hash: scope_hash.to_string(),
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(handle)
    }

    pub(super) fn validate(&self, handle: &str, scope_hash: &str) -> Result<()> {
        let mut handles = self
            .handles
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin secret handle lock is poisoned"))?;
        let now = Instant::now();
        handles.retain(|_, record| record.expires_at > now);
        let record = handles
            .get(handle)
            .ok_or_else(|| anyhow::anyhow!("Plugin secret handle is invalid or expired"))?;
        if record.scope_hash != scope_hash {
            bail!("Plugin secret handle scope does not match the requested credential");
        }
        Ok(())
    }

    pub(super) fn revoke(&self, handle: &str) -> Result<bool> {
        Ok(self
            .handles
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin secret handle lock is poisoned"))?
            .remove(handle)
            .is_some())
    }

    pub(super) fn revoke_scope(&self, scope_hash: &str) -> Result<usize> {
        let mut handles = self
            .handles
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin secret handle lock is poisoned"))?;
        let before = handles.len();
        handles.retain(|_, record| record.scope_hash != scope_hash);
        Ok(before.saturating_sub(handles.len()))
    }
}
