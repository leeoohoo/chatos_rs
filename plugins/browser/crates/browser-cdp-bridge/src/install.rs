use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_os = "windows")]
use crate::platform_data_dir;

pub const NATIVE_HOST_NAME: &str = "ai.chatos.browser_bridge";

#[derive(Debug, Clone)]
pub struct NativeHostInstallation {
    pub manifest_paths: Vec<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeHostManifest {
    name: String,
    description: String,
    path: PathBuf,
    r#type: String,
    allowed_origins: Vec<String>,
}

pub async fn install_native_host(
    executable: &Path,
    extension_id: &str,
) -> Result<NativeHostInstallation, String> {
    validate_extension_id(extension_id)?;
    let executable = executable
        .canonicalize()
        .map_err(|error| format!("could not resolve Browser MCP executable: {error}"))?;
    if !executable.is_file() {
        return Err("Browser MCP executable is not a regular file".into());
    }
    let manifest = NativeHostManifest {
        name: NATIVE_HOST_NAME.into(),
        description: "Chatos Browser MCP native messaging bootstrap".into(),
        path: executable,
        r#type: "stdio".into(),
        allowed_origins: vec![format!("chrome-extension://{extension_id}/")],
    };
    let paths = native_manifest_paths()?;
    for path in &paths {
        write_owned_manifest(path, &manifest).await?;
    }
    register_windows_manifests(paths.first()).await?;
    Ok(NativeHostInstallation {
        manifest_paths: paths,
    })
}

pub async fn uninstall_native_host(extension_id: &str) -> Result<Vec<PathBuf>, String> {
    validate_extension_id(extension_id)?;
    let expected_origin = format!("chrome-extension://{extension_id}/");
    let paths = native_manifest_paths()?;
    let mut owned_paths = Vec::new();
    for path in &paths {
        let Ok(bytes) = tokio::fs::read(path).await else {
            continue;
        };
        let manifest: NativeHostManifest = serde_json::from_slice(&bytes).map_err(|_| {
            format!(
                "refusing to remove malformed native host manifest {}",
                path.display()
            )
        })?;
        if manifest.name != NATIVE_HOST_NAME
            || manifest.allowed_origins != [expected_origin.clone()]
        {
            return Err(format!(
                "refusing to remove native host manifest not owned by this extension: {}",
                path.display()
            ));
        }
        owned_paths.push(path.clone());
    }
    for path in &owned_paths {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| format!("could not remove {}: {error}", path.display()))?;
    }
    unregister_windows_manifests().await?;
    Ok(paths)
}

async fn write_owned_manifest(path: &Path, manifest: &NativeHostManifest) -> Result<(), String> {
    if let Ok(bytes) = tokio::fs::read(path).await {
        let existing: NativeHostManifest = serde_json::from_slice(&bytes).map_err(|_| {
            format!(
                "refusing to overwrite malformed native host manifest {}",
                path.display()
            )
        })?;
        if existing.name != NATIVE_HOST_NAME {
            return Err(format!(
                "refusing to overwrite native host manifest owned by another application: {}",
                path.display()
            ));
        }
    }
    let parent = path.parent().ok_or_else(|| {
        format!(
            "native host manifest path has no parent: {}",
            path.display()
        )
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{NATIVE_HOST_NAME}.{}.tmp",
        Uuid::new_v4().simple()
    ));
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("could not serialize native host manifest: {error}"))?;
    bytes.push(b'\n');
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| format!("could not secure {}: {error}", temporary.display()))?;
    }
    if tokio::fs::try_exists(path).await.unwrap_or(false) {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| format!("could not replace {}: {error}", path.display()))?;
    }
    tokio::fs::rename(&temporary, path)
        .await
        .map_err(|error| format!("could not install {}: {error}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn native_manifest_paths() -> Result<Vec<PathBuf>, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unavailable for native host installation".to_owned())?;
    Ok(["Google/Chrome", "Chromium", "Microsoft Edge"]
        .into_iter()
        .map(|product| {
            home.join("Library/Application Support")
                .join(product)
                .join("NativeMessagingHosts")
                .join(format!("{NATIVE_HOST_NAME}.json"))
        })
        .collect())
}

#[cfg(target_os = "linux")]
fn native_manifest_paths() -> Result<Vec<PathBuf>, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unavailable for native host installation".to_owned())?;
    Ok(["google-chrome", "chromium", "microsoft-edge"]
        .into_iter()
        .map(|product| {
            home.join(".config")
                .join(product)
                .join("NativeMessagingHosts")
                .join(format!("{NATIVE_HOST_NAME}.json"))
        })
        .collect())
}

#[cfg(target_os = "windows")]
fn native_manifest_paths() -> Result<Vec<PathBuf>, String> {
    Ok(vec![
        platform_data_dir()
            .join("native-host")
            .join(format!("{NATIVE_HOST_NAME}.json")),
    ])
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn native_manifest_paths() -> Result<Vec<PathBuf>, String> {
    Err("native messaging is unsupported on this platform".into())
}

#[cfg(target_os = "windows")]
async fn register_windows_manifests(manifest: Option<&PathBuf>) -> Result<(), String> {
    let manifest =
        manifest.ok_or_else(|| "Windows native host manifest path is unavailable".to_owned())?;
    for product in ["Google\\Chrome", "Chromium", "Microsoft\\Edge"] {
        let key = format!("HKCU\\Software\\{product}\\NativeMessagingHosts\\{NATIVE_HOST_NAME}");
        let status = tokio::process::Command::new("reg.exe")
            .args(["add", &key, "/ve", "/t", "REG_SZ", "/d"])
            .arg(manifest)
            .arg("/f")
            .status()
            .await
            .map_err(|error| format!("could not run reg.exe: {error}"))?;
        if !status.success() {
            return Err(format!("could not register native host key {key}"));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
async fn register_windows_manifests(_: Option<&PathBuf>) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
async fn unregister_windows_manifests() -> Result<(), String> {
    for product in ["Google\\Chrome", "Chromium", "Microsoft\\Edge"] {
        let key = format!("HKCU\\Software\\{product}\\NativeMessagingHosts\\{NATIVE_HOST_NAME}");
        let _ = tokio::process::Command::new("reg.exe")
            .args(["delete", &key, "/f"])
            .status()
            .await;
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
async fn unregister_windows_manifests() -> Result<(), String> {
    Ok(())
}

fn validate_extension_id(extension_id: &str) -> Result<(), String> {
    if extension_id.len() == 32
        && extension_id
            .chars()
            .all(|character| matches!(character, 'a'..='p'))
    {
        Ok(())
    } else {
        Err("Chrome extension ID must contain 32 characters in the range a-p".into())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    const EXTENSION_ID: &str = "nkeimhogjdpnpccoofpliimaahmaaome";

    #[tokio::test]
    async fn writes_private_owned_manifest() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("native-host.json");
        let manifest = NativeHostManifest {
            name: NATIVE_HOST_NAME.into(),
            description: "test".into(),
            path: directory.path().join("browser-mcp"),
            r#type: "stdio".into(),
            allowed_origins: vec![format!("chrome-extension://{EXTENSION_ID}/")],
        };
        write_owned_manifest(&path, &manifest).await.unwrap();
        let persisted: NativeHostManifest =
            serde_json::from_slice(&tokio::fs::read(&path).await.unwrap()).unwrap();
        assert_eq!(persisted.name, NATIVE_HOST_NAME);
        assert_eq!(persisted.allowed_origins, manifest.allowed_origins);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = tokio::fs::metadata(path)
                .await
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }
    }

    #[tokio::test]
    async fn refuses_to_overwrite_foreign_manifest() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("native-host.json");
        tokio::fs::write(
            &path,
            serde_json::to_vec(&NativeHostManifest {
                name: "example.foreign_host".into(),
                description: "foreign".into(),
                path: directory.path().join("foreign"),
                r#type: "stdio".into(),
                allowed_origins: vec![format!("chrome-extension://{EXTENSION_ID}/")],
            })
            .unwrap(),
        )
        .await
        .unwrap();
        let replacement = NativeHostManifest {
            name: NATIVE_HOST_NAME.into(),
            description: "ours".into(),
            path: directory.path().join("browser-mcp"),
            r#type: "stdio".into(),
            allowed_origins: vec![format!("chrome-extension://{EXTENSION_ID}/")],
        };
        assert!(write_owned_manifest(&path, &replacement).await.is_err());
    }
}
