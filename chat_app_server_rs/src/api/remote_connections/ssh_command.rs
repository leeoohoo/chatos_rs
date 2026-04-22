use crate::models::remote_connection::RemoteConnection;

use super::shell_quote;

pub(super) fn build_ssh_args(
    connection: &RemoteConnection,
    interactive: bool,
    default_remote_path: Option<&str>,
) -> Vec<String> {
    let mut args = Vec::new();

    if interactive {
        args.push("-tt".to_string());
    }

    args.push("-o".to_string());
    args.push(format!(
        "BatchMode={}",
        if is_password_auth(connection) {
            "no"
        } else {
            "yes"
        }
    ));

    args.push("-o".to_string());
    args.push("ConnectTimeout=10".to_string());

    if is_password_auth(connection) {
        args.push("-o".to_string());
        args.push("PreferredAuthentications=keyboard-interactive,password".to_string());
        args.push("-o".to_string());
        args.push("KbdInteractiveAuthentication=yes".to_string());
    }

    args.push("-o".to_string());
    args.push(format!(
        "StrictHostKeyChecking={}",
        if connection.host_key_policy == "accept_new" {
            "accept-new"
        } else {
            "yes"
        }
    ));

    if !is_password_auth(connection) {
        if let Some(path) = connection.private_key_path.as_ref() {
            args.push("-i".to_string());
            args.push(path.clone());
        }

        if let Some(path) = connection.certificate_path.as_ref() {
            args.push("-o".to_string());
            args.push(format!("CertificateFile={path}"));
        }
    }

    if connection.jump_enabled {
        if let (Some(host), Some(username)) = (
            connection.jump_host.as_ref(),
            connection.jump_username.as_ref(),
        ) {
            if let Some(jump_key) = connection.jump_private_key_path.as_ref() {
                let jump_port = connection.jump_port.unwrap_or(22);
                let proxy = format!(
                    "ssh -i {}{} -p {} -W %h:%p {}@{}",
                    shell_quote(jump_key),
                    connection
                        .jump_certificate_path
                        .as_ref()
                        .map(|path| format!(" -o CertificateFile={}", shell_quote(path)))
                        .unwrap_or_default(),
                    jump_port,
                    shell_quote(username),
                    shell_quote(host)
                );
                args.push("-o".to_string());
                args.push(format!("ProxyCommand={proxy}"));
            } else {
                let mut target = format!("{username}@{host}");
                if let Some(port) = connection.jump_port {
                    target.push(':');
                    target.push_str(port.to_string().as_str());
                }
                args.push("-J".to_string());
                args.push(target);
            }
        }
    }

    args.push("-p".to_string());
    args.push(connection.port.to_string());

    args.push(format!("{}@{}", connection.username, connection.host));

    if let Some(path) = default_remote_path {
        args.push(build_remote_login_command(path));
    }

    args
}

fn build_remote_login_command(path: &str) -> String {
    let quoted = shell_quote(path);
    format!("cd {quoted} 2>/dev/null || true; exec \"${{SHELL:-/bin/bash}}\" -l")
}

pub(super) fn build_scp_args(connection: &RemoteConnection) -> Vec<String> {
    let mut args = Vec::new();

    args.push("-q".to_string());

    args.push("-o".to_string());
    args.push(format!(
        "BatchMode={}",
        if is_password_auth(connection) {
            "no"
        } else {
            "yes"
        }
    ));

    args.push("-o".to_string());
    args.push("ConnectTimeout=15".to_string());

    if is_password_auth(connection) {
        args.push("-o".to_string());
        args.push("PreferredAuthentications=keyboard-interactive,password".to_string());
        args.push("-o".to_string());
        args.push("KbdInteractiveAuthentication=yes".to_string());
    }

    args.push("-o".to_string());
    args.push(format!(
        "StrictHostKeyChecking={}",
        if connection.host_key_policy == "accept_new" {
            "accept-new"
        } else {
            "yes"
        }
    ));

    if !is_password_auth(connection) {
        if let Some(path) = connection.private_key_path.as_ref() {
            args.push("-i".to_string());
            args.push(path.clone());
        }

        if let Some(path) = connection.certificate_path.as_ref() {
            args.push("-o".to_string());
            args.push(format!("CertificateFile={path}"));
        }
    }

    if connection.jump_enabled {
        if let (Some(host), Some(username)) = (
            connection.jump_host.as_ref(),
            connection.jump_username.as_ref(),
        ) {
            if let Some(jump_key) = connection.jump_private_key_path.as_ref() {
                let jump_port = connection.jump_port.unwrap_or(22);
                let proxy = format!(
                    "ssh -i {}{} -p {} -W %h:%p {}@{}",
                    shell_quote(jump_key),
                    connection
                        .jump_certificate_path
                        .as_ref()
                        .map(|path| format!(" -o CertificateFile={}", shell_quote(path)))
                        .unwrap_or_default(),
                    jump_port,
                    shell_quote(username),
                    shell_quote(host)
                );
                args.push("-o".to_string());
                args.push(format!("ProxyCommand={proxy}"));
            } else {
                let mut target = format!("{username}@{host}");
                if let Some(port) = connection.jump_port {
                    target.push(':');
                    target.push_str(port.to_string().as_str());
                }
                args.push("-J".to_string());
                args.push(target);
            }
        }
    }

    args.push("-P".to_string());
    args.push(connection.port.to_string());

    args
}

pub(super) fn is_password_auth(connection: &RemoteConnection) -> bool {
    connection.auth_type == "password"
}

pub(super) fn build_ssh_process_command(
    connection: &RemoteConnection,
) -> Result<tokio::process::Command, String> {
    if is_password_auth(connection) {
        let password = connection
            .password
            .as_ref()
            .ok_or_else(|| "password 模式需要提供 password".to_string())?;
        let mut cmd = tokio::process::Command::new("sshpass");
        cmd.arg("-p");
        cmd.arg(password);
        cmd.arg("ssh");
        Ok(cmd)
    } else {
        Ok(tokio::process::Command::new("ssh"))
    }
}

pub(super) fn build_scp_process_command(
    connection: &RemoteConnection,
) -> Result<tokio::process::Command, String> {
    if is_password_auth(connection) {
        let password = connection
            .password
            .as_ref()
            .ok_or_else(|| "password 模式需要提供 password".to_string())?;
        let mut cmd = tokio::process::Command::new("sshpass");
        cmd.arg("-p");
        cmd.arg(password);
        cmd.arg("scp");
        Ok(cmd)
    } else {
        Ok(tokio::process::Command::new("scp"))
    }
}

pub(super) fn map_command_spawn_error(
    prefix: &str,
    error: std::io::Error,
    password_auth: bool,
) -> String {
    if password_auth && error.kind() == std::io::ErrorKind::NotFound {
        return format!("{prefix}: 未找到 sshpass，请先安装 sshpass 后再使用密码登录");
    }
    format!("{prefix}: {error}")
}
