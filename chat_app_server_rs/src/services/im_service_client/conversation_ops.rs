use super::dto::{
    ConversationActionRequestDto, ConversationMessageDto, ConversationRunDto,
    CreateConversationActionRequestDto,
    CreateConversationMessageRequestDto, CreateConversationRequestDto,
    CreateConversationRunRequestDto, ImConversationDto, UpdateConversationRequestDto,
    UpdateConversationActionRequestDto, UpdateConversationRunRequestDto,
};
use super::http::{
    build_url, client, send_json, send_json_with_service_token, timeout_duration,
};

pub async fn list_conversations() -> Result<Vec<ImConversationDto>, String> {
    let req = client()
        .get(build_url("/conversations").as_str())
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn create_conversation(
    req_body: &CreateConversationRequestDto,
) -> Result<ImConversationDto, String> {
    let req = client()
        .post(build_url("/conversations").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn get_conversation(conversation_id: &str) -> Result<ImConversationDto, String> {
    let req = client()
        .get(
            build_url(&format!("/conversations/{}", urlencoding::encode(conversation_id))).as_str(),
        )
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn update_conversation(
    conversation_id: &str,
    req_body: &UpdateConversationRequestDto,
) -> Result<ImConversationDto, String> {
    let req = client()
        .patch(
            build_url(&format!("/conversations/{}", urlencoding::encode(conversation_id))).as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn mark_conversation_read(conversation_id: &str) -> Result<ImConversationDto, String> {
    let req = client()
        .post(
            build_url(&format!(
                "/conversations/{}/read",
                urlencoding::encode(conversation_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn list_conversation_messages(
    conversation_id: &str,
    limit: Option<i64>,
    order: Option<&str>,
) -> Result<Vec<ConversationMessageDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = limit {
        params.push(("limit".to_string(), value.max(1).to_string()));
    }
    if let Some(value) = order.map(str::trim).filter(|value| !value.is_empty()) {
        params.push(("order".to_string(), value.to_string()));
    }
    let req = client()
        .get(
            build_url(&format!(
                "/conversations/{}/messages",
                urlencoding::encode(conversation_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .query(&params);
    send_json(req).await
}

pub async fn create_conversation_message(
    conversation_id: &str,
    req_body: &CreateConversationMessageRequestDto,
) -> Result<ConversationMessageDto, String> {
    let req = client()
        .post(
            build_url(&format!(
                "/conversations/{}/messages",
                urlencoding::encode(conversation_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn create_conversation_message_internal(
    conversation_id: &str,
    req_body: &CreateConversationMessageRequestDto,
) -> Result<ConversationMessageDto, String> {
    let req = client()
        .post(
            build_url(&format!(
                "/internal/conversations/{}/messages",
                urlencoding::encode(conversation_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_json_with_service_token(req).await
}

pub async fn list_action_requests(
    conversation_id: &str,
) -> Result<Vec<ConversationActionRequestDto>, String> {
    let req = client()
        .get(
            build_url(&format!(
                "/conversations/{}/action-requests",
                urlencoding::encode(conversation_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn list_runs(conversation_id: &str) -> Result<Vec<ConversationRunDto>, String> {
    let req = client()
        .get(
            build_url(&format!(
                "/conversations/{}/runs",
                urlencoding::encode(conversation_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn create_run_internal(
    req_body: &CreateConversationRunRequestDto,
) -> Result<ConversationRunDto, String> {
    let req = client()
        .post(build_url("/internal/runs").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json_with_service_token(req).await
}

pub async fn update_run_internal(
    run_id: &str,
    req_body: &UpdateConversationRunRequestDto,
) -> Result<ConversationRunDto, String> {
    let req = client()
        .patch(
            build_url(&format!("/internal/runs/{}", urlencoding::encode(run_id))).as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_json_with_service_token(req).await
}

#[allow(dead_code)]
pub async fn create_action_request_internal(
    req_body: &CreateConversationActionRequestDto,
) -> Result<ConversationActionRequestDto, String> {
    let req = client()
        .post(build_url("/internal/action-requests").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json_with_service_token(req).await
}

pub async fn get_action_request_internal(
    action_request_id: &str,
) -> Result<ConversationActionRequestDto, String> {
    let req = client()
        .get(
            build_url(&format!(
                "/internal/action-requests/{}",
                urlencoding::encode(action_request_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());
    send_json_with_service_token(req).await
}

pub async fn update_action_request_internal(
    action_request_id: &str,
    req_body: &UpdateConversationActionRequestDto,
) -> Result<ConversationActionRequestDto, String> {
    let req = client()
        .patch(
            build_url(&format!(
                "/internal/action-requests/{}",
                urlencoding::encode(action_request_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_json_with_service_token(req).await
}
