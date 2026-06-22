use std::env;
use std::path::Path;
use std::process::Stdio;

use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) struct BrowserRuntimeSession {
    pub(crate) session_name: String,
    pub(crate) cdp_url: Option<String>,
}

pub(crate) fn new_browser_session() -> BrowserRuntimeSession {
    let cdp_override = env::var("BROWSER_CDP_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    BrowserRuntimeSession {
        session_name: if cdp_override.is_some() {
            format!("cdp_{}", Uuid::new_v4().simple())
        } else {
            format!("h_{}", Uuid::new_v4().simple())
        },
        cdp_url: cdp_override,
    }
}

pub(crate) fn browser_backend_available() -> Result<(), String> {
    resolve_agent_browser_cmd().map(|_| ())
}

pub(crate) async fn run_browser_command(
    workspace_dir: &Path,
    session: &BrowserRuntimeSession,
    command: &str,
    args: Vec<String>,
    timeout_seconds: u64,
) -> Result<Value, String> {
    let (program, prefix) = resolve_agent_browser_cmd()?;
    let mut cmd = Command::new(program);
    cmd.current_dir(workspace_dir);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for value in prefix {
        cmd.arg(value);
    }

    if let Some(cdp_url) = session.cdp_url.as_deref() {
        cmd.arg("--cdp").arg(cdp_url);
    } else {
        cmd.arg("--session").arg(session.session_name.as_str());
    }
    cmd.arg("--json").arg(command);
    for value in args {
        cmd.arg(value);
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("spawn browser command failed: {}", err))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "missing browser stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "missing browser stderr".to_string())?;
    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.map(|_| buf)
    });
    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.map(|_| buf)
    });

    let status = match timeout(Duration::from_secs(timeout_seconds.max(1)), child.wait()).await {
        Ok(result) => result.map_err(|err| format!("wait browser command failed: {}", err))?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(json!({
                "success": false,
                "error": format!("Command timed out after {} seconds", timeout_seconds.max(1))
            }));
        }
    };

    let stdout = stdout_task
        .await
        .map_err(|err| format!("read browser stdout join failed: {}", err))?
        .map_err(|err| format!("read browser stdout failed: {}", err))?;
    let stderr = stderr_task
        .await
        .map_err(|err| format!("read browser stderr join failed: {}", err))?
        .map_err(|err| format!("read browser stderr failed: {}", err))?;

    let stdout_text = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
    if stdout_text.is_empty() && status.success() && command != "close" && command != "record" {
        return Ok(json!({
            "success": false,
            "error": format!("Browser command '{}' returned no output", command)
        }));
    }

    if !stdout_text.is_empty() {
        match serde_json::from_str::<Value>(&stdout_text) {
            Ok(parsed) => return Ok(parsed),
            Err(err) => {
                return Ok(json!({
                    "success": false,
                    "error": format!(
                        "Non-JSON output from agent-browser for '{}': {}",
                        command,
                        truncate_chars(&stdout_text, 2000)
                    ),
                    "detail": err.to_string(),
                }));
            }
        }
    }

    if !status.success() {
        return Ok(json!({
            "success": false,
            "error": if stderr_text.is_empty() {
                format!("Browser command failed with status {}", status)
            } else {
                stderr_text
            }
        }));
    }

    Ok(json!({
        "success": true,
        "data": {}
    }))
}

fn resolve_agent_browser_cmd() -> Result<(String, Vec<String>), String> {
    if let Some(value) = env::var("AGENT_BROWSER_BIN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return Ok((value, vec![]));
    }
    if command_exists("agent-browser") {
        return Ok(("agent-browser".to_string(), vec![]));
    }
    if command_exists("npx") {
        return Ok(("npx".to_string(), vec!["agent-browser".to_string()]));
    }
    Err(
        "agent-browser CLI not found. Install with: npm install -g agent-browser && agent-browser install"
            .to_string(),
    )
}

fn command_exists(program: &str) -> bool {
    let path_value = match env::var_os("PATH") {
        Some(value) => value,
        None => return false,
    };
    for dir in env::split_paths(&path_value) {
        let full = dir.join(program);
        if full.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            let full_exe = dir.join(format!("{}.exe", program));
            if full_exe.is_file() {
                return true;
            }
        }
    }
    false
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}
