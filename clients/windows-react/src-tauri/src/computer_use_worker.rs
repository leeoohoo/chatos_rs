use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

const DIAGNOSTIC_TIMEOUT: Duration = Duration::from_secs(8);
const MAXIMUM_DIAGNOSTIC_BYTES: u64 = 256 * 1_024;
const ONBOARDING_MAXIMUM_LIFETIME: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrustedComputerUseLaunch {
    pub(crate) executable: PathBuf,
    pub(crate) working_directory: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerUseDiagnostics {
    pub(crate) screen_capture_available: bool,
    pub(crate) input_desktop_available: bool,
    pub(crate) process_elevated: bool,
    pub(crate) all_granted: bool,
    #[serde(default)]
    pub(crate) onboarding_presented: bool,
}

pub(crate) async fn diagnose(
    launch: &TrustedComputerUseLaunch,
) -> Result<ComputerUseDiagnostics, String> {
    diagnose_with_timeout(launch, DIAGNOSTIC_TIMEOUT).await
}

async fn diagnose_with_timeout(
    launch: &TrustedComputerUseLaunch,
    maximum_duration: Duration,
) -> Result<ComputerUseDiagnostics, String> {
    validate_launch(launch)?;
    let mut command = prepared_command(launch);
    command
        .arg("--doctor")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| "Computer Use 运行检查无法启动。".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Computer Use 运行检查输出不可用。".to_owned())?;
    let collected = timeout(maximum_duration, async {
        let mut bytes = Vec::new();
        stdout
            .take(MAXIMUM_DIAGNOSTIC_BYTES + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Computer Use 运行检查输出读取失败。".to_owned())?;
        if bytes.len() as u64 > MAXIMUM_DIAGNOSTIC_BYTES {
            return Err("Computer Use 运行检查响应过大。".into());
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "Computer Use 运行检查无法完成。".to_owned())?;
        if !status.success() {
            return Err("Computer Use 运行检查失败。".into());
        }
        Ok::<_, String>(bytes)
    })
    .await;
    let bytes = match collected {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err("Computer Use 运行检查超时。".into());
        }
    };
    parse_diagnostics(&bytes)
}

pub(crate) fn open_onboarding(launch: &TrustedComputerUseLaunch) -> Result<(), String> {
    validate_launch(launch)?;
    let mut command = prepared_command(launch);
    command
        .arg("--onboarding")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    let mut child = command
        .spawn()
        .map_err(|_| "Computer Use 处理指引无法打开。".to_owned())?;
    tauri::async_runtime::spawn(async move {
        if timeout(ONBOARDING_MAXIMUM_LIFETIME, child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    });
    Ok(())
}

fn parse_diagnostics(bytes: &[u8]) -> Result<ComputerUseDiagnostics, String> {
    if bytes.is_empty() || bytes.len() as u64 > MAXIMUM_DIAGNOSTIC_BYTES {
        return Err("Computer Use 运行检查响应无效。".into());
    }
    let diagnostics = serde_json::from_slice::<ComputerUseDiagnostics>(bytes)
        .map_err(|_| "Computer Use 运行检查响应无效。".to_owned())?;
    if diagnostics.all_granted
        != (diagnostics.screen_capture_available && diagnostics.input_desktop_available)
    {
        return Err("Computer Use 运行检查响应不一致。".into());
    }
    Ok(diagnostics)
}

fn validate_launch(launch: &TrustedComputerUseLaunch) -> Result<(), String> {
    let executable = launch
        .executable
        .canonicalize()
        .map_err(|_| "Computer Use 可执行文件不可用。".to_owned())?;
    let working_directory = launch
        .working_directory
        .canonicalize()
        .map_err(|_| "Computer Use 安装目录不可用。".to_owned())?;
    if executable != launch.executable
        || working_directory != launch.working_directory
        || !executable.is_file()
        || !working_directory.is_dir()
        || !executable.starts_with(&working_directory)
    {
        return Err("Computer Use 安装边界无效。".into());
    }
    Ok(())
}

fn prepared_command(launch: &TrustedComputerUseLaunch) -> Command {
    let mut command = Command::new(&launch.executable);
    command
        .current_dir(&launch.working_directory)
        .env_clear()
        .envs(safe_environment());
    command
}

fn safe_environment() -> BTreeMap<String, String> {
    let mut environment = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "ComSpec", "PATHEXT", "TEMP", "TMP"] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    environment
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_consistent_bounded_capability_results() {
        let value = br#"{
          "screenCaptureAvailable": true,
          "inputDesktopAvailable": true,
          "processElevated": false,
          "allGranted": true,
          "onboardingPresented": false,
          "applicationName": "Visual Computer Use",
          "permissions": [],
          "guidance": []
        }"#;
        let parsed = parse_diagnostics(value).unwrap();
        assert!(parsed.all_granted);
        assert!(!parsed.process_elevated);

        let inconsistent = br#"{
          "screenCaptureAvailable": false,
          "inputDesktopAvailable": true,
          "processElevated": false,
          "allGranted": true
        }"#;
        assert!(parse_diagnostics(inconsistent).is_err());
        assert!(parse_diagnostics(&vec![b'x'; MAXIMUM_DIAGNOSTIC_BYTES as usize + 1]).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn doctor_uses_the_verified_executable_and_times_out_closed() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let executable = root.join("computer-use-fixture");
        std::fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = \"--doctor\" ]; then printf '%s\\n' '{\"screenCaptureAvailable\":true,\"inputDesktopAvailable\":true,\"processElevated\":false,\"allGranted\":true}'; fi\n",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let launch = TrustedComputerUseLaunch {
            executable: executable.canonicalize().unwrap(),
            working_directory: root,
        };
        assert!(
            diagnose_with_timeout(&launch, Duration::from_secs(1))
                .await
                .unwrap()
                .all_granted
        );

        std::fs::write(&executable, "#!/bin/sh\nexec sleep 2\n").unwrap();
        assert!(diagnose_with_timeout(&launch, Duration::from_millis(50))
            .await
            .unwrap_err()
            .contains("超时"));
    }

    #[test]
    fn refuses_executables_outside_the_verified_installation_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let launch = TrustedComputerUseLaunch {
            executable: outside.path().canonicalize().unwrap(),
            working_directory: root,
        };
        assert!(validate_launch(&launch).is_err());
    }
}
