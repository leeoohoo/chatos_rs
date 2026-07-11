// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use lettre::message::{Mailbox, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::AppConfig;

pub async fn send_registration_code(
    config: &AppConfig,
    email: &str,
    code: &str,
) -> Result<(), String> {
    let host = config
        .smtp_host
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "USER_SERVICE_SMTP_HOST is required".to_string())?;
    let username = config
        .smtp_username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "USER_SERVICE_SMTP_USERNAME is required".to_string())?;
    let password = config
        .smtp_password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "USER_SERVICE_SMTP_PASSWORD is required".to_string())?;
    let from_address = config
        .email_from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(username);

    let from: Mailbox = format!("{} <{}>", config.email_from_name, from_address)
        .parse()
        .map_err(|err| format!("invalid USER_SERVICE_EMAIL_FROM: {err}"))?;
    let to: Mailbox = email
        .parse()
        .map_err(|err| format!("invalid recipient email: {err}"))?;
    let body = format!(
        "Your ChatOS registration verification code is: {code}\n\n\
         This code expires in 10 minutes. Do not share it with anyone.\n\n\
         ChatOS"
    );
    let message = Message::builder()
        .from(from)
        .to(to)
        .subject("ChatOS registration verification code")
        .singlepart(SinglePart::plain(body))
        .map_err(|err| format!("build registration email failed: {err}"))?;
    let credentials = Credentials::new(username.to_string(), password.to_string());
    let transport_builder = if config.smtp_port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
    }
    .map_err(|err| format!("build SMTP transport failed: {err}"))?;
    let mailer = transport_builder
        .port(config.smtp_port)
        .credentials(credentials)
        .build();
    mailer
        .send(message)
        .await
        .map(|_| ())
        .map_err(|err| format!("send registration email failed: {err}"))
}
