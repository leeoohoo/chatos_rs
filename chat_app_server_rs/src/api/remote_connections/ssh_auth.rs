use ssh2::{KeyboardInteractivePrompt, Prompt, Session};
use std::path::Path as FsPath;

use crate::models::remote_connection::RemoteConnection;

const SECOND_FACTOR_REQUIRED_ERROR_PREFIX: &str = "__CHATOS_SECOND_FACTOR_REQUIRED__:";
const PASSWORD_PROMPT_HINTS: &[&str] = &["password", "passphrase", "passcode", "密码"];
const SECOND_FACTOR_PROMPT_HINTS: &[&str] = &[
    "otp",
    "verification code",
    "one-time",
    "two-factor",
    "2fa",
    "mfa",
    "token",
    "验证码",
    "动态码",
    "一次性",
    "手机",
];
const PROMPT_TEXT_PREVIEW_LIMIT: usize = 80;

fn prompt_contains_hint(prompt: &str, hints: &[&str]) -> bool {
    let normalized = prompt.to_lowercase();
    hints.iter().any(|hint| normalized.contains(hint))
}

fn is_password_prompt_text(prompt: &str) -> bool {
    prompt_contains_hint(prompt, PASSWORD_PROMPT_HINTS)
}

fn is_second_factor_prompt_text(prompt: &str) -> bool {
    if is_password_prompt_text(prompt) {
        return false;
    }
    prompt_contains_hint(prompt, SECOND_FACTOR_PROMPT_HINTS)
}

fn compact_prompt_text(prompt: &str) -> String {
    let compact = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = compact.chars().count();
    if count <= PROMPT_TEXT_PREVIEW_LIMIT {
        return compact;
    }
    compact.chars().take(PROMPT_TEXT_PREVIEW_LIMIT).collect()
}

struct PasswordKeyboardPrompter {
    password: String,
    verification_code: Option<String>,
    password_used: bool,
    verification_code_used: bool,
    second_factor_prompts: Vec<String>,
}

impl PasswordKeyboardPrompter {
    fn new(password: &str, verification_code: Option<&str>) -> Self {
        Self {
            password: password.to_string(),
            verification_code: verification_code
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned),
            password_used: false,
            verification_code_used: false,
            second_factor_prompts: Vec::new(),
        }
    }

    fn saw_second_factor_challenge(&self) -> bool {
        !self.second_factor_prompts.is_empty()
    }

    fn prompt_summary(&self) -> String {
        if self.second_factor_prompts.is_empty() {
            return "验证码提示".to_string();
        }
        self.second_factor_prompts
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<String>>()
            .join(" / ")
    }
}

impl KeyboardInteractivePrompt for PasswordKeyboardPrompter {
    fn prompt<'a>(
        &mut self,
        _username: &str,
        _instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        prompts
            .iter()
            .map(|item| {
                let text = item.text.trim();
                if is_password_prompt_text(text)
                    || (!self.password_used && !item.echo && !is_second_factor_prompt_text(text))
                {
                    self.password_used = true;
                    return self.password.clone();
                }
                if is_second_factor_prompt_text(text) {
                    self.second_factor_prompts.push(compact_prompt_text(text));
                    if let Some(code) = self.verification_code.as_ref() {
                        self.verification_code_used = true;
                        return code.clone();
                    }
                }
                String::new()
            })
            .collect()
    }
}

pub(super) fn encode_second_factor_required_error(prompt: &str) -> String {
    format!("{SECOND_FACTOR_REQUIRED_ERROR_PREFIX}{prompt}")
}

pub(super) fn extract_second_factor_required_prompt(error: &str) -> Option<String> {
    error
        .strip_prefix(SECOND_FACTOR_REQUIRED_ERROR_PREFIX)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn authenticate_with_password_fallbacks(
    session: &Session,
    username: &str,
    password: &str,
    verification_code: Option<&str>,
    target_label: &str,
) -> Result<(), String> {
    let mut failures = Vec::new();

    match session.userauth_password(username, password) {
        Ok(_) => return Ok(()),
        Err(err) => failures.push(format!("password: {err}")),
    }

    let mut prompter = PasswordKeyboardPrompter::new(password, verification_code);
    match session.userauth_keyboard_interactive(username, &mut prompter) {
        Ok(_) => return Ok(()),
        Err(err) => {
            if prompter.saw_second_factor_challenge() {
                let prompt_summary = prompter.prompt_summary();
                if verification_code
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .is_none()
                    || !prompter.verification_code_used
                {
                    return Err(encode_second_factor_required_error(prompt_summary.as_str()));
                }
                return Err(encode_second_factor_required_error(prompt_summary.as_str()));
            } else {
                failures.push(format!("keyboard-interactive: {err}"));
            }
        }
    }

    Err(format!("{target_label}失败: {}", failures.join("；")))
}

pub(super) fn authenticate_target_session(
    session: &Session,
    connection: &RemoteConnection,
    verification_code: Option<&str>,
) -> Result<(), String> {
    match connection.auth_type.as_str() {
        "password" => {
            let password = connection
                .password
                .as_ref()
                .ok_or_else(|| "password 模式需要提供 password".to_string())?;
            authenticate_with_password_fallbacks(
                session,
                connection.username.as_str(),
                password.as_str(),
                verification_code,
                "密码认证",
            )?;
        }
        "private_key" | "private_key_cert" => {
            let private_key = connection
                .private_key_path
                .as_ref()
                .ok_or_else(|| "私钥路径不能为空".to_string())?;
            let cert_path = connection.certificate_path.as_ref().map(FsPath::new);
            session
                .userauth_pubkey_file(
                    connection.username.as_str(),
                    cert_path,
                    FsPath::new(private_key),
                    None,
                )
                .map_err(|e| format!("密钥认证失败: {e}"))?;
        }
        _ => return Err("不支持的认证方式".to_string()),
    }
    Ok(())
}

pub(super) fn authenticate_jump_session(
    session: &Session,
    connection: &RemoteConnection,
    jump_username: &str,
    verification_code: Option<&str>,
) -> Result<(), String> {
    let mut failures = Vec::new();

    if let Some(jump_key_path) = connection.jump_private_key_path.as_ref() {
        match session.userauth_pubkey_file(jump_username, None, FsPath::new(jump_key_path), None) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("jump_private_key_path 认证失败: {err}")),
        }
    }

    if let Some(jump_password) = connection.jump_password.as_ref() {
        match authenticate_with_password_fallbacks(
            session,
            jump_username,
            jump_password.as_str(),
            verification_code,
            "jump_password 认证",
        ) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("jump_password 认证失败: {err}")),
        }
    }

    if connection.auth_type != "password" {
        if let Some(private_key_path) = connection.private_key_path.as_ref() {
            let cert_path = connection.certificate_path.as_ref().map(FsPath::new);
            match session.userauth_pubkey_file(
                jump_username,
                cert_path,
                FsPath::new(private_key_path),
                None,
            ) {
                Ok(_) => return Ok(()),
                Err(err) => failures.push(format!("复用目标密钥认证失败: {err}")),
            }
        }
    }

    match session.userauth_agent(jump_username) {
        Ok(_) => return Ok(()),
        Err(err) => failures.push(format!("SSH Agent 认证失败: {err}")),
    }

    if let Some(password) = connection.password.as_ref() {
        match authenticate_with_password_fallbacks(
            session,
            jump_username,
            password.as_str(),
            verification_code,
            "复用目标密码认证",
        ) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("使用同密码认证失败: {err}")),
        }
    }

    if failures.is_empty() {
        return Err("跳板机认证失败".to_string());
    }

    Err(format!(
        "跳板机认证失败：{}。请配置 jump_private_key_path、jump_password 或 SSH Agent",
        failures.join("；")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn detects_password_prompt_hint() {
        assert!(is_password_prompt_text("Password:"));
        assert!(is_password_prompt_text("请输入密码"));
        assert!(!is_password_prompt_text("Verification code"));
    }

    #[test]
    fn detects_second_factor_prompt_hint() {
        assert!(is_second_factor_prompt_text("Verification code:"));
        assert!(is_second_factor_prompt_text("请输入手机验证码"));
        assert!(!is_second_factor_prompt_text("Password:"));
    }

    #[test]
    fn keyboard_prompter_records_second_factor_prompt() {
        let mut prompter = PasswordKeyboardPrompter::new("secret", None);
        let prompts = vec![
            Prompt {
                text: Cow::Borrowed("Password: "),
                echo: false,
            },
            Prompt {
                text: Cow::Borrowed("SMS verification code: "),
                echo: false,
            },
        ];

        let responses = prompter.prompt("", "", prompts.as_slice());
        assert_eq!(responses, vec!["secret".to_string(), "".to_string()]);
        assert!(prompter.saw_second_factor_challenge());
        assert!(prompter.prompt_summary().contains("verification code"));
    }

    #[test]
    fn encodes_and_extracts_second_factor_required_error() {
        let encoded = encode_second_factor_required_error("SMS verification code");
        let parsed = extract_second_factor_required_prompt(encoded.as_str());
        assert_eq!(parsed.as_deref(), Some("SMS verification code"));
    }
}
