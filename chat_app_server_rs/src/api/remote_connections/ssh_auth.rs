use ssh2::{KeyboardInteractivePrompt, Prompt, Session};
use std::path::Path as FsPath;
use std::sync::mpsc;
use std::time::Duration as StdDuration;
use tracing::warn;

use crate::models::remote_connection::RemoteConnection;

const DEFAULT_SECOND_FACTOR_PROMPT: &str = "请输入验证码 / OTP";
const SECOND_FACTOR_REQUIRED_ERROR_PREFIX: &str = "__CHATOS_SECOND_FACTOR_REQUIRED__:";
const PASSWORD_PROMPT_HINTS: &[&str] = &["password", "passphrase", "密码"];
const SECOND_FACTOR_PROMPT_HINTS: &[&str] = &[
    "otp",
    "passcode",
    "verification code",
    "authentication code",
    "security code",
    "sms",
    "one-time",
    "two-factor",
    "2fa",
    "mfa",
    "token",
    "code",
    "验证码",
    "校验码",
    "认证码",
    "动态码",
    "动态口令",
    "一次性",
    "手机",
    "短信",
];
const PROMPT_TEXT_PREVIEW_LIMIT: usize = 80;
const SECOND_FACTOR_WAIT_TIMEOUT: StdDuration = StdDuration::from_secs(120);

fn has_verification_code(verification_code: Option<&str>) -> bool {
    verification_code
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
}

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
    verification_code_rx: Option<mpsc::Receiver<String>>,
    challenge_tx: Option<mpsc::Sender<String>>,
    password_used: bool,
    verification_code_used: bool,
    second_factor_prompts: Vec<String>,
}

impl PasswordKeyboardPrompter {
    fn new_with_channels(
        password: &str,
        verification_code: Option<&str>,
        verification_code_rx: Option<mpsc::Receiver<String>>,
        challenge_tx: Option<mpsc::Sender<String>>,
    ) -> Self {
        Self {
            password: password.to_string(),
            verification_code: verification_code
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned),
            verification_code_rx,
            challenge_tx,
            password_used: false,
            verification_code_used: false,
            second_factor_prompts: Vec::new(),
        }
    }

    fn saw_second_factor_challenge(&self) -> bool {
        !self.second_factor_prompts.is_empty()
    }

    fn note_second_factor_prompt(&mut self, prompt: &str) {
        let compact = compact_prompt_text(prompt);
        let preview = if compact.is_empty() {
            DEFAULT_SECOND_FACTOR_PROMPT.to_string()
        } else {
            compact
        };
        if !self.second_factor_prompts.contains(&preview) {
            self.second_factor_prompts.push(preview);
        }
    }

    fn prompt_summary(&self) -> String {
        if self.second_factor_prompts.is_empty() {
            return DEFAULT_SECOND_FACTOR_PROMPT.to_string();
        }
        self.second_factor_prompts
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<String>>()
            .join(" / ")
    }

    fn next_verification_code(&mut self) -> Option<String> {
        if let Some(code) = self.verification_code.as_ref() {
            self.verification_code_used = true;
            return Some(code.clone());
        }

        let prompt_summary = self.prompt_summary();
        if let Some(challenge_tx) = self.challenge_tx.as_ref() {
            let _ = challenge_tx.send(prompt_summary);
        }

        let rx = self.verification_code_rx.as_ref()?;
        match rx.recv_timeout(SECOND_FACTOR_WAIT_TIMEOUT) {
            Ok(code) => {
                let trimmed = code.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    self.verification_code_used = true;
                    Some(trimmed)
                }
            }
            Err(err) => {
                warn!(error = %err, "Timed out waiting for remote terminal verification code");
                None
            }
        }
    }
}

impl KeyboardInteractivePrompt for PasswordKeyboardPrompter {
    fn prompt<'a>(
        &mut self,
        _username: &str,
        instructions: &str,
        prompts: &[Prompt<'a>],
    ) -> Vec<String> {
        let instructions = instructions.trim();
        if is_second_factor_prompt_text(instructions) {
            self.note_second_factor_prompt(instructions);
        }

        prompts
            .iter()
            .map(|item| {
                let text = item.text.trim();
                let saw_second_factor_hint = self.saw_second_factor_challenge();
                if is_password_prompt_text(text)
                    || (!self.password_used
                        && !item.echo
                        && !saw_second_factor_hint
                        && !is_second_factor_prompt_text(text))
                {
                    self.password_used = true;
                    return self.password.clone();
                }
                if is_second_factor_prompt_text(text)
                    || (self.password_used && !is_password_prompt_text(text))
                {
                    self.note_second_factor_prompt(text);
                    if let Some(code) = self.next_verification_code() {
                        return code;
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
    let marker_index = error.find(SECOND_FACTOR_REQUIRED_ERROR_PREFIX)?;
    let prompt_with_context = &error[marker_index + SECOND_FACTOR_REQUIRED_ERROR_PREFIX.len()..];
    prompt_with_context
        .split(['；', ';', '。', '\n', '\r'])
        .next()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn authenticate_with_password_fallbacks(
    session: &Session,
    username: &str,
    password: &str,
    verification_code: Option<&str>,
    verification_code_rx: Option<mpsc::Receiver<String>>,
    challenge_tx: Option<mpsc::Sender<String>>,
    target_label: &str,
) -> Result<(), String> {
    let mut failures = Vec::new();

    match session.userauth_password(username, password) {
        Ok(_) => {
            if session.authenticated() {
                return Ok(());
            }
            warn!(
                target_label = target_label,
                username = username,
                "SSH password auth completed but session still requires additional authentication"
            );
        }
        Err(err) => failures.push(format!("password: {err}")),
    }

    let mut prompter = PasswordKeyboardPrompter::new_with_channels(
        password,
        verification_code,
        verification_code_rx,
        challenge_tx,
    );
    match session.userauth_keyboard_interactive(username, &mut prompter) {
        Ok(_) => {
            if session.authenticated() {
                return Ok(());
            }
            let prompt_summary = prompter.prompt_summary();
            if prompter.verification_code_used {
                warn!(
                    target_label = target_label,
                    prompt = prompt_summary.as_str(),
                    "SSH verification code was submitted but authentication did not complete"
                );
                return Err(format!("{target_label}失败: 验证码认证失败或验证码已过期"));
            }
            if prompter.saw_second_factor_challenge() {
                return Err(encode_second_factor_required_error(prompt_summary.as_str()));
            }
            failures.push("keyboard-interactive: session not authenticated".to_string());
        }
        Err(err) => {
            if prompter.verification_code_used {
                warn!(
                    target_label = target_label,
                    error = %err,
                    prompt = prompter.prompt_summary().as_str(),
                    "SSH verification code was rejected"
                );
                return Err(format!("{target_label}失败: 验证码认证失败或验证码已过期"));
            }
            if prompter.saw_second_factor_challenge() {
                let prompt_summary = prompter.prompt_summary();
                warn!(
                    target_label = target_label,
                    error = %err,
                    prompt = prompt_summary.as_str(),
                    password_used = prompter.password_used,
                    verification_code_used = prompter.verification_code_used,
                    has_verification_code = has_verification_code(verification_code),
                    "SSH keyboard-interactive requested second-factor verification"
                );
                return Err(encode_second_factor_required_error(prompt_summary.as_str()));
            }
            failures.push(format!("keyboard-interactive: {err}"));
        }
    }

    if failures.is_empty() {
        Err(format!("{target_label}失败: SSH 会话未完成认证"))
    } else {
        Err(format!("{target_label}失败: {}", failures.join("；")))
    }
}

pub(super) fn authenticate_target_session(
    session: &Session,
    connection: &RemoteConnection,
    verification_code: Option<&str>,
    verification_code_rx: Option<mpsc::Receiver<String>>,
    challenge_tx: Option<mpsc::Sender<String>>,
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
                verification_code_rx,
                challenge_tx,
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
    verification_code_rx: Option<mpsc::Receiver<String>>,
    challenge_tx: Option<mpsc::Sender<String>>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    let mut verification_code_rx = verification_code_rx;
    let mut challenge_tx = challenge_tx;

    if let Some(jump_key_path) = connection.jump_private_key_path.as_ref() {
        let jump_cert_path = connection.jump_certificate_path.as_ref().map(FsPath::new);
        match session.userauth_pubkey_file(
            jump_username,
            jump_cert_path,
            FsPath::new(jump_key_path),
            None,
        ) {
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
            verification_code_rx.take(),
            challenge_tx.take(),
            "jump_password 认证",
        ) {
            Ok(_) => return Ok(()),
            Err(err) => {
                if extract_second_factor_required_prompt(err.as_str()).is_some() {
                    return Err(err);
                }
                failures.push(format!("jump_password 认证失败: {err}"));
            }
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
            verification_code_rx.take(),
            challenge_tx.take(),
            "复用目标密码认证",
        ) {
            Ok(_) => return Ok(()),
            Err(err) => {
                if extract_second_factor_required_prompt(err.as_str()).is_some() {
                    return Err(err);
                }
                failures.push(format!("使用同密码认证失败: {err}"));
            }
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
        assert!(is_second_factor_prompt_text("Passcode:"));
        assert!(!is_second_factor_prompt_text("Password:"));
    }

    #[test]
    fn keyboard_prompter_records_second_factor_prompt() {
        let mut prompter =
            PasswordKeyboardPrompter::new_with_channels("secret", None, None, None);
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
    fn keyboard_prompter_inferrs_unknown_follow_up_prompt_as_second_factor() {
        let mut prompter =
            PasswordKeyboardPrompter::new_with_channels("secret", None, None, None);
        let prompts = vec![
            Prompt {
                text: Cow::Borrowed("Password: "),
                echo: false,
            },
            Prompt {
                text: Cow::Borrowed("请输入阿里云堡垒机动态口令: "),
                echo: false,
            },
        ];

        let responses = prompter.prompt("", "", prompts.as_slice());
        assert_eq!(responses, vec!["secret".to_string(), "".to_string()]);
        assert!(prompter.saw_second_factor_challenge());
        assert!(prompter.prompt_summary().contains("动态口令"));
    }

    #[test]
    fn keyboard_prompter_uses_verification_code_for_blank_follow_up_prompt() {
        let mut prompter =
            PasswordKeyboardPrompter::new_with_channels("secret", Some("123456"), None, None);
        let prompts = vec![
            Prompt {
                text: Cow::Borrowed("Password: "),
                echo: false,
            },
            Prompt {
                text: Cow::Borrowed(""),
                echo: false,
            },
        ];

        let responses = prompter.prompt("", "", prompts.as_slice());
        assert_eq!(responses, vec!["secret".to_string(), "123456".to_string()]);
        assert!(prompter.saw_second_factor_challenge());
        assert!(prompter.verification_code_used);
    }

    #[test]
    fn encodes_and_extracts_second_factor_required_error() {
        let encoded = encode_second_factor_required_error("SMS verification code");
        let parsed = extract_second_factor_required_prompt(encoded.as_str());
        assert_eq!(parsed.as_deref(), Some("SMS verification code"));
    }

    #[test]
    fn extracts_second_factor_prompt_from_wrapped_error() {
        let encoded = encode_second_factor_required_error("SMS verification code");
        let wrapped = format!("跳板机认证失败：jump_password 认证失败: {encoded}。请检查配置");
        let parsed = extract_second_factor_required_prompt(wrapped.as_str());
        assert_eq!(parsed.as_deref(), Some("SMS verification code"));
    }
}
