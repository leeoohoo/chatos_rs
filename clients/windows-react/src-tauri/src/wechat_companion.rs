use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::ImageReader;
use serde::{Deserialize, Serialize};
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};
use tauri::{http, State};
use uuid::Uuid;

use crate::{authentication::AuthenticationManager, host_lifecycle::validate_owner};

const MAXIMUM_QR_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_QR_PIXELS: u64 = 4_000_000;

#[derive(Clone, Default)]
pub(crate) struct WeChatCompanionManager {
    ticket: Arc<Mutex<Option<ActiveBindTicket>>>,
}

#[derive(Clone)]
struct ActiveBindTicket {
    owner_user_id: String,
    authentication_generation: u64,
    ticket_id: String,
    status: String,
    expires_at_unix: i64,
    preview_token: String,
    content_type: &'static str,
    png: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WeChatBindingStatus {
    bound: bool,
    #[serde(alias = "created_at")]
    created_at: Option<String>,
    #[serde(alias = "last_login_at")]
    last_login_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WeChatClientSession {
    id: String,
    #[serde(alias = "client_type")]
    client_type: String,
    #[serde(alias = "created_at")]
    created_at: String,
    #[serde(alias = "last_seen_at")]
    last_seen_at: String,
    #[serde(alias = "expires_at_unix")]
    expires_at_unix: i64,
    #[serde(alias = "revoked_at")]
    revoked_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeChatCompanionSnapshot {
    binding: WeChatBindingStatus,
    sessions: Vec<WeChatClientSession>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeChatBindTicketProjection {
    status: String,
    expires_at_unix: i64,
    preview_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IssueBindTicketResponse {
    ticket_id: String,
    bind_ticket: String,
    scene: String,
    expires_in_seconds: i64,
    expires_at_unix: i64,
    qr_code_data_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindTicketStatusResponse {
    ticket_id: String,
    status: String,
    expires_at_unix: i64,
    claimed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmBindTicketResponse {
    status: String,
    external_identity_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RevokeWeChatSessionInput {
    owner_user_id: String,
    session_id: String,
}

impl WeChatCompanionManager {
    fn publish(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
        response: IssueBindTicketResponse,
    ) -> Result<WeChatBindTicketProjection, String> {
        validate_uuid(&response.ticket_id)?;
        validate_text(&response.scene, 512, false)?;
        validate_text(&response.bind_ticket, 512, false)?;
        let now = chrono::Utc::now().timestamp();
        if response.expires_at_unix <= now || response.expires_at_unix > now + 900 {
            return Err(invalid_ticket());
        }
        if response.expires_in_seconds <= 0 || response.expires_in_seconds > 900 {
            return Err(invalid_ticket());
        }
        let (png, content_type) = decode_qr_image(&response.qr_code_data_url)?;
        let preview_token = Uuid::new_v4().to_string().to_lowercase();
        let active = ActiveBindTicket {
            owner_user_id: owner_user_id.to_owned(),
            authentication_generation,
            ticket_id: response.ticket_id,
            status: "issued".into(),
            expires_at_unix: response.expires_at_unix,
            preview_token: preview_token.clone(),
            content_type,
            png,
        };
        let projection = project_ticket(&active);
        *self.ticket.lock().map_err(|_| companion_error())? = Some(active);
        Ok(projection)
    }

    fn active(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
    ) -> Result<ActiveBindTicket, String> {
        let ticket = self.ticket.lock().map_err(|_| companion_error())?;
        let active = ticket.as_ref().ok_or_else(stale_ticket)?;
        if active.owner_user_id != owner_user_id
            || active.authentication_generation != authentication_generation
        {
            return Err(stale_ticket());
        }
        Ok(active.clone())
    }

    fn accept_status(
        &self,
        expected: &ActiveBindTicket,
        response: BindTicketStatusResponse,
    ) -> Result<WeChatBindTicketProjection, String> {
        if response.ticket_id != expected.ticket_id
            || response.expires_at_unix != expected.expires_at_unix
            || !matches!(response.status.as_str(), "issued" | "claimed" | "expired")
        {
            return Err(invalid_ticket());
        }
        if response.status == "claimed" {
            validate_optional_text(response.claimed_at.as_deref(), 128)?;
        }
        let mut ticket = self.ticket.lock().map_err(|_| companion_error())?;
        let active = ticket.as_mut().ok_or_else(stale_ticket)?;
        if active.preview_token != expected.preview_token || active.ticket_id != expected.ticket_id
        {
            return Err(stale_ticket());
        }
        active.status = response.status;
        Ok(project_ticket(active))
    }

    fn clear(&self) {
        if let Ok(mut ticket) = self.ticket.lock() {
            *ticket = None;
        }
    }

    fn clear_for(&self, owner_user_id: &str, authentication_generation: u64) {
        if let Ok(mut ticket) = self.ticket.lock() {
            if ticket.as_ref().is_some_and(|active| {
                active.owner_user_id == owner_user_id
                    && active.authentication_generation == authentication_generation
            }) {
                *ticket = None;
            }
        }
    }

    pub(crate) fn reset(&self) {
        self.clear();
    }

    fn protocol_response(&self, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
        let token = request.uri().path().trim_matches('/');
        let body = self.ticket.lock().ok().and_then(|ticket| {
            ticket
                .as_ref()
                .filter(|active| active.preview_token == token)
                .map(|active| (active.content_type, active.png.clone()))
        });
        match body {
            Some((content_type, body)) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, content_type)
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(body)
                .expect("valid WeChat QR response"),
            None => http::Response::builder()
                .status(http::StatusCode::NOT_FOUND)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"WeChat QR unavailable".to_vec())
                .expect("valid WeChat QR error response"),
        }
    }
}

#[tauri::command]
pub(crate) async fn load_wechat_companion(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
) -> Result<WeChatCompanionSnapshot, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let (binding, sessions) = tokio::try_join!(
        authentication.authenticated_user_service::<WeChatBindingStatus>(
            &owner_user_id,
            "auth/wechat/mini-program/binding",
            "GET",
        ),
        authentication.authenticated_user_service::<Vec<WeChatClientSession>>(
            &owner_user_id,
            "auth/client-sessions",
            "GET",
        ),
    )?;
    validate_binding(&binding)?;
    validate_sessions(&sessions)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(WeChatCompanionSnapshot { binding, sessions })
}

#[tauri::command]
pub(crate) async fn issue_wechat_bind_ticket(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, WeChatCompanionManager>,
) -> Result<WeChatBindTicketProjection, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let response = authentication
        .authenticated_user_service::<IssueBindTicketResponse>(
            &owner_user_id,
            "auth/wechat/mini-program/bind-tickets",
            "POST",
        )
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    manager.publish(&owner_user_id, generation, response)
}

#[tauri::command]
pub(crate) async fn poll_wechat_bind_ticket(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, WeChatCompanionManager>,
) -> Result<WeChatBindTicketProjection, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let active = manager.active(&owner_user_id, generation)?;
    if chrono::Utc::now().timestamp() >= active.expires_at_unix {
        return manager.accept_status(
            &active,
            BindTicketStatusResponse {
                ticket_id: active.ticket_id.clone(),
                status: "expired".into(),
                expires_at_unix: active.expires_at_unix,
                claimed_at: None,
            },
        );
    }
    let response = authentication
        .authenticated_user_service::<BindTicketStatusResponse>(
            &owner_user_id,
            &format!("auth/wechat/mini-program/bind-tickets/{}", active.ticket_id),
            "GET",
        )
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    manager.accept_status(&active, response)
}

#[tauri::command]
pub(crate) async fn confirm_wechat_bind_ticket(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, WeChatCompanionManager>,
) -> Result<(), String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let active = manager.active(&owner_user_id, generation)?;
    if active.status != "claimed" || chrono::Utc::now().timestamp() >= active.expires_at_unix {
        return Err(stale_ticket());
    }
    let response = authentication
        .authenticated_user_service::<ConfirmBindTicketResponse>(
            &owner_user_id,
            &format!(
                "auth/wechat/mini-program/bind-tickets/{}/confirm",
                active.ticket_id
            ),
            "POST",
        )
        .await?;
    if response.status != "confirmed" {
        return Err(invalid_ticket());
    }
    validate_uuid(&response.external_identity_id)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    manager.clear();
    Ok(())
}

#[tauri::command]
pub(crate) async fn revoke_wechat_client_session(
    input: RevokeWeChatSessionInput,
    authentication: State<'_, AuthenticationManager>,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    validate_uuid(&input.session_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    authentication
        .authenticated_user_service::<()>(
            &input.owner_user_id,
            &format!("auth/client-sessions/{}", input.session_id),
            "DELETE",
        )
        .await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await
}

#[tauri::command]
pub(crate) async fn unbind_wechat_companion(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, WeChatCompanionManager>,
) -> Result<(), String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .authenticated_user_service::<()>(
            &owner_user_id,
            "auth/wechat/mini-program/binding",
            "DELETE",
        )
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    manager.clear();
    Ok(())
}

#[tauri::command]
pub(crate) async fn discard_wechat_bind_ticket(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, WeChatCompanionManager>,
) -> Result<(), String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    manager.clear_for(&owner_user_id, generation);
    Ok(())
}

pub(crate) fn wechat_qr_protocol(
    manager: WeChatCompanionManager,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    manager.protocol_response(request)
}

fn project_ticket(ticket: &ActiveBindTicket) -> WeChatBindTicketProjection {
    let preview_url = if cfg!(target_os = "windows") {
        format!("http://chatos-wechat.localhost/{}", ticket.preview_token)
    } else {
        format!("chatos-wechat://localhost/{}", ticket.preview_token)
    };
    WeChatBindTicketProjection {
        status: ticket.status.clone(),
        expires_at_unix: ticket.expires_at_unix,
        preview_url,
    }
}

fn decode_qr_image(value: &str) -> Result<(Vec<u8>, &'static str), String> {
    let (encoded, expected_format, content_type) =
        if let Some(value) = value.strip_prefix("data:image/png;base64,") {
            (value, image::ImageFormat::Png, "image/png")
        } else if let Some(value) = value.strip_prefix("data:image/jpeg;base64,") {
            (value, image::ImageFormat::Jpeg, "image/jpeg")
        } else {
            return Err(invalid_ticket());
        };
    if encoded.len() > MAXIMUM_QR_BYTES * 2 {
        return Err(invalid_ticket());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| invalid_ticket())?;
    if bytes.is_empty()
        || bytes.len() > MAXIMUM_QR_BYTES
        || image::guess_format(&bytes).ok() != Some(expected_format)
    {
        return Err(invalid_ticket());
    }
    let (width, height) = ImageReader::with_format(Cursor::new(&bytes), expected_format)
        .into_dimensions()
        .map_err(|_| invalid_ticket())?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAXIMUM_QR_PIXELS {
        return Err(invalid_ticket());
    }
    Ok((bytes, content_type))
}

fn validate_binding(value: &WeChatBindingStatus) -> Result<(), String> {
    validate_optional_text(value.created_at.as_deref(), 128)?;
    validate_optional_text(value.last_login_at.as_deref(), 128)
}

fn validate_sessions(values: &[WeChatClientSession]) -> Result<(), String> {
    if values.len() > 1_000 {
        return Err(companion_error());
    }
    for value in values {
        validate_uuid(&value.id)?;
        if value.client_type != "wechat_mini_program" || value.expires_at_unix <= 0 {
            return Err(companion_error());
        }
        validate_text(&value.created_at, 128, false)?;
        validate_text(&value.last_seen_at, 128, false)?;
        validate_optional_text(value.revoked_at.as_deref(), 128)?;
    }
    Ok(())
}

fn validate_uuid(value: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| companion_error())
}

fn validate_optional_text(value: Option<&str>, maximum: usize) -> Result<(), String> {
    value.map_or(Ok(()), |value| validate_text(value, maximum, false))
}

fn validate_text(value: &str, maximum: usize, allow_empty: bool) -> Result<(), String> {
    if (!allow_empty && value.is_empty())
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        Err(companion_error())
    } else {
        Ok(())
    }
}

fn invalid_ticket() -> String {
    "微信绑定二维码无效，请重新生成。".into()
}
fn stale_ticket() -> String {
    "微信绑定请求已经变化，请重新生成。".into()
}
fn companion_error() -> String {
    "微信 Companion 返回了无效数据。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn qr_data_url() -> String {
        let image = ImageBuffer::from_pixel(2, 2, Rgba([0_u8, 0, 0, 255]));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            STANDARD.encode(bytes.into_inner())
        )
    }

    #[test]
    fn ticket_projection_hides_server_identity_and_serves_only_the_active_token() {
        let manager = WeChatCompanionManager::default();
        let projection = manager
            .publish(
                "alice",
                7,
                IssueBindTicketResponse {
                    ticket_id: Uuid::new_v4().to_string(),
                    bind_ticket: "secret-scene".into(),
                    scene: "secret-scene".into(),
                    expires_in_seconds: 120,
                    expires_at_unix: chrono::Utc::now().timestamp() + 120,
                    qr_code_data_url: qr_data_url(),
                },
            )
            .unwrap();
        assert!(!serde_json::to_string(&projection)
            .unwrap()
            .contains("secret-scene"));
        let response = manager.protocol_response(
            http::Request::builder()
                .uri(&projection.preview_url)
                .body(Vec::new())
                .unwrap(),
        );
        assert_eq!(response.status(), http::StatusCode::OK);
        manager.reset();
        let response = manager.protocol_response(
            http::Request::builder()
                .uri(&projection.preview_url)
                .body(Vec::new())
                .unwrap(),
        );
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn rejects_invalid_ticket_payloads() {
        assert!(decode_qr_image("data:image/png;base64,dGVzdA==").is_err());
        assert!(decode_qr_image("data:image/gif;base64,dGVzdA==").is_err());
    }

    #[test]
    fn decodes_the_current_user_service_wire_contract() {
        let ticket: IssueBindTicketResponse = serde_json::from_value(serde_json::json!({
            "ticket_id": Uuid::new_v4().to_string(),
            "bind_ticket": "secret",
            "scene": "secret",
            "expires_in_seconds": 120,
            "expires_at_unix": chrono::Utc::now().timestamp() + 120,
            "qr_code_data_url": qr_data_url(),
        }))
        .unwrap();
        assert_eq!(ticket.expires_in_seconds, 120);

        let session: WeChatClientSession = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "client_type": "wechat_mini_program",
            "created_at": "2026-10-22T00:00:00Z",
            "last_seen_at": "2026-10-22T00:00:00Z",
            "expires_at_unix": 4_102_444_800_i64,
            "revoked_at": null,
        }))
        .unwrap();
        assert_eq!(session.client_type, "wechat_mini_program");
    }
}
