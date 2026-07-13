// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn normalize_username(value: &str) -> Result<String, String> {
    let username = value.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err("用户名不能为空".to_string());
    }
    if username.len() > 64 {
        return Err("用户名不能超过 64 个字符".to_string());
    }
    Ok(username)
}

pub(super) fn normalize_display_name(value: &str, username: &str) -> String {
    let display_name = value.trim();
    if display_name.is_empty() {
        username.to_string()
    } else {
        display_name.to_string()
    }
}

pub(super) fn hash_password(password: &str) -> Result<String, String> {
    if password.trim().is_empty() {
        return Err("密码不能为空".to_string());
    }
    let mut salt_bytes = [0_u8; 16];
    rand::fill(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|err| err.to_string())?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| err.to_string())
}

pub(super) fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}
