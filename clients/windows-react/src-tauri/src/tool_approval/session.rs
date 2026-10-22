use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

pub(crate) struct ApprovalSessionScope {
    pub(crate) owner_user_id: String,
    pub(crate) device_id: String,
    pub(crate) workspace_id: String,
    pub(crate) project_root: PathBuf,
    pub(crate) cwd: PathBuf,
    pub(crate) base: String,
    pub(crate) purpose: Option<String>,
    pub(crate) requested_permissions_description: String,
    pub(crate) command: String,
    pub(crate) arguments: Vec<String>,
}

impl ApprovalSessionScope {
    pub(crate) fn key(&self) -> String {
        let project_root = self.project_root.to_string_lossy();
        let cwd = self.cwd.to_string_lossy();
        let mut values = vec![
            self.owner_user_id.as_str(),
            self.device_id.as_str(),
            self.workspace_id.as_str(),
            project_root.as_ref(),
            cwd.as_ref(),
            self.base.as_str(),
            self.purpose.as_deref().unwrap_or_default(),
            self.requested_permissions_description.as_str(),
        ];
        values.push(self.command.as_str());
        values.extend(self.arguments.iter().map(String::as_str));
        let mut canonical = String::new();
        for value in values {
            canonical.push_str(&value.len().to_string());
            canonical.push(':');
            canonical.push_str(value);
        }
        format!("v2:{:x}", Sha256::digest(canonical.as_bytes()))
    }
}

#[derive(Default)]
struct ApprovalSessionState {
    generation: u64,
    keys: HashSet<String>,
}

#[derive(Clone, Default)]
pub(crate) struct ApprovalSessionAllowlist {
    state: Arc<Mutex<ApprovalSessionState>>,
}

impl ApprovalSessionAllowlist {
    pub(crate) fn generation(&self) -> u64 {
        self.lock().generation
    }

    pub(crate) fn contains(&self, generation: u64, key: &str) -> bool {
        let state = self.lock();
        state.generation == generation && state.keys.contains(key)
    }

    pub(crate) fn insert_if_current(&self, generation: u64, key: String) -> bool {
        let mut state = self.lock();
        if state.generation != generation {
            return false;
        }
        state.keys.insert(key);
        true
    }

    pub(crate) fn reset(&self) {
        let mut state = self.lock();
        state.generation = state.generation.wrapping_add(1);
        state.keys.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lock().keys.is_empty()
    }

    fn lock(&self) -> MutexGuard<'_, ApprovalSessionState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_request_scope_is_stable_and_bound_to_every_security_dimension() {
        let baseline = scope().key();
        assert_eq!(
            baseline,
            "v2:afec857ae85eb89bc3e31ad2ec5b5dbb92d0d3685254af5fd38d185de91d0375"
        );
        assert_eq!(baseline, scope().key());

        let mut variants = Vec::new();
        variants.push(ApprovalSessionScope {
            owner_user_id: "owner-2".into(),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            device_id: "device-2".into(),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            workspace_id: "workspace-2".into(),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            project_root: PathBuf::from("C:/project/other"),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            cwd: PathBuf::from("C:/project/subdirectory"),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            base: "local-agent-terminal:conversation-2".into(),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            purpose: Some("另一项检查".into()),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            requested_permissions_description: "另一项权限".into(),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            command: "pwsh.exe".into(),
            ..scope()
        });
        variants.push(ApprovalSessionScope {
            arguments: vec!["-Command".into(), "cargo clippy".into()],
            ..scope()
        });

        for variant in variants {
            assert_ne!(baseline, variant.key());
        }
    }

    #[test]
    fn reset_clears_keys_and_invalidates_in_flight_insertions() {
        let allowlist = ApprovalSessionAllowlist::default();
        let generation = allowlist.generation();
        assert!(allowlist.insert_if_current(generation, scope().key()));
        assert!(!allowlist.is_empty());
        assert!(allowlist.contains(generation, &scope().key()));

        allowlist.reset();
        assert!(allowlist.is_empty());
        assert!(!allowlist.contains(generation, &scope().key()));
        assert!(!allowlist.insert_if_current(generation, scope().key()));
    }

    fn scope() -> ApprovalSessionScope {
        ApprovalSessionScope {
            owner_user_id: "owner-1".into(),
            device_id: "device-1".into(),
            workspace_id: "workspace-1".into(),
            project_root: PathBuf::from("C:/project"),
            cwd: PathBuf::from("C:/project"),
            base: "local-agent-terminal:conversation-1".into(),
            purpose: Some("验证当前改动".into()),
            requested_permissions_description: "在当前项目中执行命令".into(),
            command: "powershell.exe".into(),
            arguments: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-Command".into(),
                "cargo test".into(),
            ],
        }
    }
}
