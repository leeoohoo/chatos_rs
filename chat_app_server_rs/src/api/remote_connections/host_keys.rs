use std::path::Path as FsPath;

use ssh2::{CheckResult, KnownHostFileKind, KnownHostKeyFormat, Session};

fn known_hosts_file_path() -> Result<std::path::PathBuf, String> {
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

fn host_key_record_name(host: &str, port: i64) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{}]:{}", host, port)
    }
}

fn replace_known_host_entry(
    known_hosts: &mut ssh2::KnownHosts,
    known_hosts_path: &FsPath,
    host: &str,
    port: i64,
    host_key: &[u8],
    host_key_type: ssh2::HostKeyType,
) -> Result<(), String> {
    let mut aliases = vec![host_key_record_name(host, port)];
    let plain_host = host.to_string();
    if !aliases.iter().any(|item| item == &plain_host) {
        aliases.push(plain_host);
    }

    for entry in known_hosts
        .hosts()
        .map_err(|e| format!("读取 known_hosts 条目失败: {e}"))?
    {
        if let Some(name) = entry.name() {
            if aliases.iter().any(|alias| alias == name) {
                known_hosts
                    .remove(&entry)
                    .map_err(|e| format!("更新 known_hosts 失败: {e}"))?;
            }
        }
    }

    let key_format = host_key_format_from_ssh2(host_key_type)?;
    let host_for_add = host_key_record_name(host, port);
    known_hosts
        .add(host_for_add.as_str(), host_key, "", key_format)
        .map_err(|e| format!("写入 known_hosts 失败: {e}"))?;
    known_hosts
        .write_file(known_hosts_path, KnownHostFileKind::OpenSSH)
        .map_err(|e| format!("保存 known_hosts 失败: {e}"))?;
    Ok(())
}

pub(super) fn apply_host_key_policy(
    session: &Session,
    host: &str,
    port: i64,
    host_key_policy: &str,
) -> Result<(), String> {
    let (host_key, host_key_type) = session
        .host_key()
        .ok_or_else(|| "远端未返回主机公钥".to_string())?;
    let mut known_hosts = session
        .known_hosts()
        .map_err(|e| format!("读取 known_hosts 失败: {e}"))?;

    let known_hosts_path = known_hosts_file_path()?;
    if known_hosts_path.exists() {
        known_hosts
            .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
            .map_err(|e| format!("加载 known_hosts 失败: {e}"))?;
    }

    let check_result = known_hosts.check_port(host, port as u16, host_key);
    match check_result {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch if host_key_policy == "accept_new" => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建 ~/.ssh 目录失败: {e}"))?;
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
        CheckResult::Mismatch => Err(
            "主机指纹与 known_hosts 记录不匹配，请核对服务器或切换 accept_new 后重试".to_string(),
        ),
        CheckResult::NotFound if host_key_policy == "accept_new" => {
            if let Some(parent) = known_hosts_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建 ~/.ssh 目录失败: {e}"))?;
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
        CheckResult::NotFound => {
            Err("主机指纹未受信任，请先加入 known_hosts 或使用 accept_new".to_string())
        }
        CheckResult::Failure => Err("主机指纹校验失败".to_string()),
    }
}
