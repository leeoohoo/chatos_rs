use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[path = "policy_paths.rs"]
mod policy_paths;
#[path = "policy_roots.rs"]
mod policy_roots;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;

pub(super) const PATH_OUTSIDE_ALLOWED_ROOTS: &str = "路径超出允许范围";
pub(super) const PATH_TRAVERSAL_BLOCKED: &str = "路径不能包含 ..";
pub(super) const ROOT_MUTATION_BLOCKED: &str = "不允许修改受控根目录";
pub(super) const WRITE_NOT_ALLOWED: &str = "当前目录不允许写入";
pub(super) use self::policy_paths::normalize_path_for_compare;

#[derive(Debug, Clone)]
pub(super) struct FsAllowedRoot {
    path: PathBuf,
    kind: FsAllowedRootKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FsAllowedRootKind {
    CurrentDir,
    Workspace,
    Project,
    ProjectParent,
    RepoParent,
    Ssh,
    Home,
    Configured,
}

impl FsAllowedRootKind {
    fn priority(self) -> u8 {
        match self {
            Self::CurrentDir => 0,
            Self::Workspace => 1,
            Self::Project => 2,
            Self::ProjectParent => 3,
            Self::RepoParent => 4,
            Self::Ssh => 5,
            Self::Home => 6,
            Self::Configured => 7,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::CurrentDir => "current_dir",
            Self::Workspace => "workspace",
            Self::Project => "project",
            Self::ProjectParent => "project_parent",
            Self::RepoParent => "repo_parent",
            Self::Ssh => "ssh",
            Self::Home => "home",
            Self::Configured => "configured",
        }
    }

    fn can_write(self) -> bool {
        matches!(
            self,
            Self::CurrentDir
                | Self::Workspace
                | Self::Project
                | Self::ProjectParent
                | Self::RepoParent
                | Self::Configured
        )
    }
}

#[derive(Debug, Clone)]
pub(super) struct FsPathPolicy {
    roots: Vec<FsAllowedRoot>,
}

#[derive(Debug, Clone)]
pub(super) struct AuthorizedPath {
    pub(super) path: PathBuf,
    pub(super) navigation_root: PathBuf,
    pub(super) project_root: Option<PathBuf>,
    pub(super) can_write: bool,
}

#[derive(Debug, Clone)]
pub(super) enum FsPolicyError {
    BadRequest(String),
    Forbidden(String),
    Internal(String),
}

impl FsPolicyError {
    pub(super) fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(super) fn message(&self) -> &str {
        match self {
            Self::BadRequest(message) | Self::Forbidden(message) | Self::Internal(message) => {
                message.as_str()
            }
        }
    }
}

impl FsPathPolicy {
    pub(super) async fn for_user(auth: &AuthUser) -> Result<Self, FsPolicyError> {
        let roots = policy_roots::build_allowed_roots(auth).await;

        if roots.is_empty() {
            return Err(FsPolicyError::Forbidden(
                "当前用户没有可访问的本地目录".to_string(),
            ));
        }

        Ok(Self { roots })
    }

    pub(super) fn roots_json(&self) -> Vec<Value> {
        self.roots
            .iter()
            .map(|root| {
                let path = root.path.to_string_lossy().to_string();
                json!({
                    "name": path,
                    "path": path,
                    "is_dir": true,
                    "kind": root.kind.as_str(),
                    "writable": root.kind.can_write(),
                })
            })
            .collect()
    }

    pub(super) fn authorize_existing_path(
        &self,
        raw: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let resolved = policy_paths::resolve_input_path(raw)?;
        let canonical = policy_paths::canonicalize_existing_path(resolved.as_path(), "路径不存在")?;
        self.authorized_path_for(canonical)
    }

    pub(super) fn authorize_existing_dir(
        &self,
        raw: &str,
        missing_message: &str,
        not_dir_message: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let authorized = self.authorize_existing_path_with_message(raw, missing_message)?;
        if !authorized.path.is_dir() {
            return Err(FsPolicyError::BadRequest(not_dir_message.to_string()));
        }
        Ok(authorized)
    }

    pub(super) fn authorize_existing_file(
        &self,
        raw: &str,
        missing_message: &str,
        not_file_message: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let authorized = self.authorize_existing_path_with_message(raw, missing_message)?;
        if !authorized.path.is_file() {
            return Err(FsPolicyError::BadRequest(not_file_message.to_string()));
        }
        Ok(authorized)
    }

    pub(super) fn authorize_existing_entry(
        &self,
        raw: &str,
        missing_message: &str,
        invalid_message: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let resolved = policy_paths::resolve_input_path(raw)?;
        let metadata = match std::fs::symlink_metadata(&resolved) {
            Ok(value) => value,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Err(FsPolicyError::BadRequest(missing_message.to_string()));
            }
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                return Err(FsPolicyError::Forbidden(
                    PATH_OUTSIDE_ALLOWED_ROOTS.to_string(),
                ));
            }
            Err(err) => return Err(FsPolicyError::Internal(err.to_string())),
        };

        if metadata.file_type().is_symlink() {
            let parent = resolved
                .parent()
                .ok_or_else(|| FsPolicyError::BadRequest(invalid_message.to_string()))?;
            let canonical_parent =
                policy_paths::canonicalize_existing_path(parent, invalid_message)?;
            let file_name = resolved
                .file_name()
                .ok_or_else(|| FsPolicyError::BadRequest(invalid_message.to_string()))?;

            return self.authorized_path_for(canonical_parent.join(file_name));
        }

        let canonical =
            policy_paths::canonicalize_existing_path(resolved.as_path(), missing_message)?;
        if self.find_exact_allowed_root(canonical.as_path()).is_some() {
            return self.authorized_path_for(canonical);
        }

        let parent = resolved
            .parent()
            .ok_or_else(|| FsPolicyError::BadRequest(invalid_message.to_string()))?;
        let canonical_parent = policy_paths::canonicalize_existing_path(parent, invalid_message)?;
        let file_name = resolved
            .file_name()
            .ok_or_else(|| FsPolicyError::BadRequest(invalid_message.to_string()))?;

        self.authorized_path_for(canonical_parent.join(file_name))
    }

    pub(super) fn forbid_root_mutation(&self, path: &Path) -> Result<(), FsPolicyError> {
        if self.is_exact_allowed_root(path) {
            return Err(FsPolicyError::Forbidden(ROOT_MUTATION_BLOCKED.to_string()));
        }
        Ok(())
    }

    pub(super) fn require_write(&self, path: &AuthorizedPath) -> Result<(), FsPolicyError> {
        if !path.can_write {
            return Err(FsPolicyError::Forbidden(WRITE_NOT_ALLOWED.to_string()));
        }
        Ok(())
    }

    pub(super) fn parent_for(&self, path: &AuthorizedPath) -> Option<String> {
        if policy_paths::normalize_path_for_compare(path.path.as_path())
            == policy_paths::normalize_path_for_compare(path.navigation_root.as_path())
        {
            return None;
        }
        let parent = path.path.parent()?;
        if !policy_paths::path_is_within_root(parent, path.navigation_root.as_path()) {
            return None;
        }
        Some(parent.to_string_lossy().to_string())
    }

    fn authorize_existing_path_with_message(
        &self,
        raw: &str,
        missing_message: &str,
    ) -> Result<AuthorizedPath, FsPolicyError> {
        let resolved = policy_paths::resolve_input_path(raw)?;
        let canonical =
            policy_paths::canonicalize_existing_path(resolved.as_path(), missing_message)?;
        self.authorized_path_for(canonical)
    }

    fn authorized_path_for(&self, path: PathBuf) -> Result<AuthorizedPath, FsPolicyError> {
        let root = self
            .find_navigation_root(path.as_path())
            .ok_or_else(|| FsPolicyError::Forbidden(PATH_OUTSIDE_ALLOWED_ROOTS.to_string()))?;
        Ok(AuthorizedPath {
            navigation_root: root.path.clone(),
            project_root: self
                .find_project_root(path.as_path())
                .map(|project_root| project_root.path.clone()),
            path,
            can_write: root.kind.can_write(),
        })
    }

    fn find_navigation_root(&self, candidate: &Path) -> Option<&FsAllowedRoot> {
        self.roots
            .iter()
            .filter(|root| policy_paths::path_is_within_root(candidate, root.path.as_path()))
            .max_by_key(|root| policy_paths::normalize_path_for_compare(root.path.as_path()).len())
    }

    fn find_exact_allowed_root(&self, candidate: &Path) -> Option<&FsAllowedRoot> {
        let normalized = policy_paths::normalize_path_for_compare(candidate);
        self.roots.iter().find(|root| {
            policy_paths::normalize_path_for_compare(root.path.as_path()) == normalized
        })
    }

    fn find_project_root(&self, candidate: &Path) -> Option<&FsAllowedRoot> {
        self.roots
            .iter()
            .filter(|root| root.kind == FsAllowedRootKind::Project)
            .filter(|root| policy_paths::path_is_within_root(candidate, root.path.as_path()))
            .max_by_key(|root| policy_paths::normalize_path_for_compare(root.path.as_path()).len())
    }

    fn is_exact_allowed_root(&self, candidate: &Path) -> bool {
        self.find_exact_allowed_root(candidate).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::policy_paths::normalize_path_for_compare;
    use super::FsPathPolicy;
    use crate::core::auth::AuthUser;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn make_temp_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("{}_{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn configured_policy(root: &Path) -> FsPathPolicy {
        FsPathPolicy {
            roots: vec![super::FsAllowedRoot {
                path: fs::canonicalize(root).expect("canonicalize root"),
                kind: super::FsAllowedRootKind::Configured,
            }],
        }
    }

    fn mock_auth() -> AuthUser {
        AuthUser {
            user_id: "tester".to_string(),
            role: "user".to_string(),
        }
    }

    #[tokio::test]
    async fn authorize_existing_path_rejects_parent_traversal() {
        let policy = FsPathPolicy::for_user(&mock_auth()).await;
        if let Ok(policy) = policy {
            let result = policy.authorize_existing_path("../outside");
            assert!(result.is_err());
        }
    }

    #[test]
    fn normalize_path_compare_trims_trailing_slash() {
        assert_eq!(
            normalize_path_for_compare(PathBuf::from("/tmp/demo/").as_path()),
            "/tmp/demo"
        );
    }

    #[test]
    fn compare_normalization_preserves_root() {
        assert_eq!(
            normalize_path_for_compare(PathBuf::from("/").as_path()),
            "/"
        );
    }

    #[test]
    fn exact_allowed_root_detection_uses_normalized_paths() {
        let root = make_temp_dir("fs_policy_root");
        let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
        let policy = configured_policy(root.as_path());
        assert!(policy.is_exact_allowed_root(root.join(".").as_path()) == false);
        assert!(policy.is_exact_allowed_root(canonical_root.as_path()));
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn parent_for_returns_navigation_root_for_nested_directory() {
        let root = make_temp_dir("fs_policy_parent");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create child");
        let policy = configured_policy(root.as_path());
        let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
        let canonical_child = fs::canonicalize(&child).expect("canonicalize child");
        let path = super::AuthorizedPath {
            path: canonical_child,
            navigation_root: canonical_root.clone(),
            project_root: Some(canonical_root.clone()),
            can_write: true,
        };

        assert_eq!(
            policy.parent_for(&path).as_deref(),
            Some(canonical_root.to_string_lossy().as_ref())
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn navigation_root_prefers_most_specific_allowed_root() {
        let root = make_temp_dir("fs_policy_specific_root");
        let nested_root = root.join(".ssh");
        let nested_child = nested_root.join("config");
        fs::create_dir_all(&nested_child).expect("create nested child");

        let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
        let canonical_nested_root =
            fs::canonicalize(&nested_root).expect("canonicalize nested root");
        let canonical_nested_child =
            fs::canonicalize(&nested_child).expect("canonicalize nested child");

        let policy = FsPathPolicy {
            roots: vec![
                super::FsAllowedRoot {
                    path: canonical_root,
                    kind: super::FsAllowedRootKind::Home,
                },
                super::FsAllowedRoot {
                    path: canonical_nested_root.clone(),
                    kind: super::FsAllowedRootKind::Ssh,
                },
            ],
        };

        let authorized = policy
            .authorize_existing_dir(
                canonical_nested_child.to_string_lossy().as_ref(),
                "路径不存在",
                "路径不是目录",
            )
            .expect("authorize nested dir");
        assert_eq!(authorized.navigation_root, canonical_nested_root);

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn write_permission_depends_on_allowed_root_kind() {
        let root = make_temp_dir("fs_policy_write_root");
        let writable = root.join("workspace");
        let readonly = root.join(".ssh");
        fs::create_dir_all(&writable).expect("create writable root");
        fs::create_dir_all(&readonly).expect("create readonly root");

        let canonical_writable = fs::canonicalize(&writable).expect("canonicalize writable");
        let canonical_readonly = fs::canonicalize(&readonly).expect("canonicalize readonly");
        let policy = FsPathPolicy {
            roots: vec![
                super::FsAllowedRoot {
                    path: canonical_writable.clone(),
                    kind: super::FsAllowedRootKind::Workspace,
                },
                super::FsAllowedRoot {
                    path: canonical_readonly.clone(),
                    kind: super::FsAllowedRootKind::Ssh,
                },
            ],
        };

        let writable_path = policy
            .authorize_existing_dir(
                canonical_writable.to_string_lossy().as_ref(),
                "路径不存在",
                "路径不是目录",
            )
            .expect("authorize writable");
        let readonly_path = policy
            .authorize_existing_dir(
                canonical_readonly.to_string_lossy().as_ref(),
                "路径不存在",
                "路径不是目录",
            )
            .expect("authorize readonly");

        assert!(policy.require_write(&writable_path).is_ok());
        assert!(matches!(
            policy.require_write(&readonly_path),
            Err(super::FsPolicyError::Forbidden(_))
        ));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn forbid_root_mutation_blocks_exact_allowed_root_only() {
        let root = make_temp_dir("fs_policy_forbid_root");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create child");
        let policy = configured_policy(root.as_path());
        let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
        let canonical_child = fs::canonicalize(&child).expect("canonicalize child");

        assert!(policy
            .forbid_root_mutation(canonical_root.as_path())
            .is_err());
        assert!(policy
            .forbid_root_mutation(canonical_child.as_path())
            .is_ok());

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[cfg(unix)]
    #[test]
    fn authorize_existing_path_rejects_symlink_escape_for_read_access() {
        use std::os::unix::fs::symlink;

        let root = make_temp_dir("fs_policy_symlink_root");
        let outside = make_temp_dir("fs_policy_symlink_outside");
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");
        let link = root.join("secret-link");
        symlink(&outside_file, &link).expect("create symlink");

        let policy = configured_policy(root.as_path());

        let result = policy.authorize_existing_path(link.to_string_lossy().as_ref());
        assert!(matches!(result, Err(super::FsPolicyError::Forbidden(_))));

        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }

    #[cfg(unix)]
    #[test]
    fn authorize_existing_entry_allows_symlink_itself_for_mutation() {
        use std::os::unix::fs::symlink;

        let root = make_temp_dir("fs_policy_entry_root");
        let outside = make_temp_dir("fs_policy_entry_outside");
        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");
        let link = root.join("secret-link");
        symlink(&outside_file, &link).expect("create symlink");

        let policy = configured_policy(root.as_path());
        let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
        let expected_link = canonical_root.join("secret-link");

        let authorized = policy
            .authorize_existing_entry(link.to_string_lossy().as_ref(), "路径不存在", "路径不合法")
            .expect("authorize symlink entry");
        assert_eq!(authorized.path, expected_link);
        assert_eq!(authorized.navigation_root, canonical_root);

        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }
}
