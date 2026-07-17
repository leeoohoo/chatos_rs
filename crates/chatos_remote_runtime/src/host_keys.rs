// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ssh2::{CheckResult, KnownHostFileKind, KnownHostKeyFormat, Session};

use crate::is_valid_ssh_port;

static KNOWN_HOSTS_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyPolicyDecision {
    TrustExisting,
    TrustAndRecord,
    Reject(&'static str),
}

pub fn evaluate_host_key_policy(
    check_result: CheckResult,
    host_key_policy: &str,
) -> HostKeyPolicyDecision {
    match check_result {
        CheckResult::Match => HostKeyPolicyDecision::TrustExisting,
        CheckResult::Mismatch => {
            HostKeyPolicyDecision::Reject("主机指纹与 known_hosts 记录不匹配，请核对服务器后重试")
        }
        CheckResult::NotFound if host_key_policy == "accept_new" => {
            HostKeyPolicyDecision::TrustAndRecord
        }
        CheckResult::NotFound => HostKeyPolicyDecision::Reject(
            "主机指纹未受信任，请先加入 known_hosts 或使用 accept_new",
        ),
        CheckResult::Failure => HostKeyPolicyDecision::Reject("主机指纹校验失败"),
    }
}

pub fn apply_host_key_policy(
    session: &Session,
    host: &str,
    port: i64,
    host_key_policy: &str,
) -> Result<(), String> {
    if !is_valid_ssh_port(port) {
        return Err("SSH 端口必须位于 1-65535".to_string());
    }
    let port = port as u16;
    let _guard = KNOWN_HOSTS_LOCK
        .lock()
        .map_err(|_| "known_hosts 写入锁不可用".to_string())?;
    let (host_key, host_key_type) = session
        .host_key()
        .ok_or_else(|| "远端未返回主机公钥".to_string())?;
    let mut known_hosts = session
        .known_hosts()
        .map_err(|err| format!("读取 known_hosts 失败: {err}"))?;
    let known_hosts_path = known_hosts_file_path()?;
    if known_hosts_path.exists() {
        known_hosts
            .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
            .map_err(|err| format!("加载 known_hosts 失败: {err}"))?;
    }

    match evaluate_host_key_policy(
        known_hosts.check_port(host, port, host_key),
        host_key_policy,
    ) {
        HostKeyPolicyDecision::TrustExisting => Ok(()),
        HostKeyPolicyDecision::TrustAndRecord => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("创建 ~/.ssh 目录失败: {err}"))?;
            }
            replace_known_host_entry(
                &mut known_hosts,
                &known_hosts_path,
                host,
                port,
                host_key,
                host_key_type,
            )
        }
        HostKeyPolicyDecision::Reject(message) => Err(message.to_string()),
    }
}

fn known_hosts_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法定位用户 home 目录".to_string())?;
    Ok(home.join(".ssh").join("known_hosts"))
}

fn host_key_format_from_ssh2(key_type: ssh2::HostKeyType) -> Result<KnownHostKeyFormat, String> {
    match key_type {
        ssh2::HostKeyType::Rsa => Ok(KnownHostKeyFormat::SshRsa),
        ssh2::HostKeyType::Dss => Ok(KnownHostKeyFormat::SshDss),
        ssh2::HostKeyType::Ecdsa256 => Ok(KnownHostKeyFormat::Ecdsa256),
        ssh2::HostKeyType::Ecdsa384 => Ok(KnownHostKeyFormat::Ecdsa384),
        ssh2::HostKeyType::Ecdsa521 => Ok(KnownHostKeyFormat::Ecdsa521),
        ssh2::HostKeyType::Ed25519 => Ok(KnownHostKeyFormat::Ed25519),
        ssh2::HostKeyType::Unknown => Err("不支持的主机公钥类型".to_string()),
    }
}

fn host_key_record_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

fn replace_known_host_entry(
    known_hosts: &mut ssh2::KnownHosts,
    known_hosts_path: &Path,
    host: &str,
    port: u16,
    host_key: &[u8],
    host_key_type: ssh2::HostKeyType,
) -> Result<(), String> {
    let record_name = host_key_record_name(host, port);
    for entry in known_hosts
        .hosts()
        .map_err(|err| format!("读取 known_hosts 条目失败: {err}"))?
    {
        if entry.name() == Some(record_name.as_str()) {
            known_hosts
                .remove(&entry)
                .map_err(|err| format!("更新 known_hosts 失败: {err}"))?;
        }
    }

    let key_format = host_key_format_from_ssh2(host_key_type)?;
    known_hosts
        .add(record_name.as_str(), host_key, "", key_format)
        .map_err(|err| format!("写入 known_hosts 失败: {err}"))?;
    known_hosts
        .write_file(known_hosts_path, KnownHostFileKind::OpenSSH)
        .map_err(|err| format!("保存 known_hosts 失败: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_new_records_unknown_hosts() {
        assert_eq!(
            evaluate_host_key_policy(CheckResult::NotFound, "accept_new"),
            HostKeyPolicyDecision::TrustAndRecord
        );
    }

    #[test]
    fn accept_new_never_overwrites_mismatched_hosts() {
        assert_eq!(
            evaluate_host_key_policy(CheckResult::Mismatch, "accept_new"),
            HostKeyPolicyDecision::Reject("主机指纹与 known_hosts 记录不匹配，请核对服务器后重试")
        );
    }

    #[test]
    fn strict_rejects_unknown_hosts() {
        assert_eq!(
            evaluate_host_key_policy(CheckResult::NotFound, "strict"),
            HostKeyPolicyDecision::Reject(
                "主机指纹未受信任，请先加入 known_hosts 或使用 accept_new"
            )
        );
    }

    #[test]
    fn matching_host_keys_are_trusted() {
        assert_eq!(
            evaluate_host_key_policy(CheckResult::Match, "strict"),
            HostKeyPolicyDecision::TrustExisting
        );
    }

    #[test]
    fn nonstandard_ports_use_bracketed_records_without_aliasing_port_22() {
        assert_eq!(host_key_record_name("example.com", 22), "example.com");
        assert_eq!(
            host_key_record_name("example.com", 2222),
            "[example.com]:2222"
        );
    }
}
