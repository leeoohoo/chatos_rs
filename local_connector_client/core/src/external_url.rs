// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};

const MAX_EXTERNAL_URL_BYTES: usize = 16 * 1024;
const EXTERNAL_OPENER_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) async fn open_external_url(value: &str) -> Result<()> {
    let url = validated_external_url(value)?;
    let mut command = external_url_command(url.as_str());
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = tokio::time::timeout(EXTERNAL_OPENER_TIMEOUT, command.status())
        .await
        .context("system URL opener timed out")??;
    if !status.success() {
        bail!("system URL opener exited unsuccessfully");
    }
    Ok(())
}

fn validated_external_url(value: &str) -> Result<reqwest::Url> {
    if value.is_empty() || value.len() > MAX_EXTERNAL_URL_BYTES {
        bail!("external URL is missing or oversized");
    }
    let url = reqwest::Url::parse(value).context("parse external URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("external URL must be an HTTP(S) URL without embedded credentials");
    }
    Ok(url)
}

fn external_url_command(url: &str) -> tokio::process::Command {
    match std::env::consts::OS {
        "macos" => {
            let mut command = tokio::process::Command::new("open");
            command.args(["--", url]);
            command
        }
        "windows" => {
            let mut command = tokio::process::Command::new("rundll32.exe");
            command.args(["url.dll,FileProtocolHandler", url]);
            command
        }
        _ => {
            let mut command = tokio::process::Command::new("xdg-open");
            command.arg(url);
            command
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validated_external_url;

    #[test]
    fn only_plain_http_urls_can_reach_the_system_opener() {
        assert!(validated_external_url("https://accounts.example.com/authorize?state=abc").is_ok());
        assert!(validated_external_url("http://127.0.0.1:17823/").is_ok());
        assert!(validated_external_url("file:///tmp/secret").is_err());
        assert!(validated_external_url("https://user:secret@example.com/").is_err());
    }
}
