use serde_json::Value;

use super::dto::{
    CreateMemoryContactRequestDto, CreateMemoryContactResponseDto, MemoryAgentRecallDto,
    MemoryContactDto, MemoryProjectAgentLinkDto, MemoryProjectContactDto, MemoryProjectDto,
    MemoryProjectMemoryDto, SyncMemoryProjectRequestDto, SyncProjectAgentLinkRequestDto,
};
use super::http::{
    client, push_limit_offset_params, send_delete_result, send_json, send_list,
    send_optional_json, try_build_url, try_timeout_duration,
};

pub async fn list_memory_contacts(
    user_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryContactDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = user_id {
        params.push(("user_id".to_string(), value.to_string()));
    }
    push_limit_offset_params(&mut params, limit, offset);

    send_list("/contacts", &params).await
}

pub async fn get_memory_contact(contact_id: &str) -> Result<Option<MemoryContactDto>, String> {
    let req = client()
        .get(try_build_url(&format!(
            "/contacts/{}",
            urlencoding::encode(contact_id)
        ))?)
        .timeout(try_timeout_duration()?);
    send_optional_json(req).await
}

pub async fn create_memory_contact(
    payload: &CreateMemoryContactRequestDto,
) -> Result<CreateMemoryContactResponseDto, String> {
    let req = client()
        .post(try_build_url("/contacts")?)
        .timeout(try_timeout_duration()?)
        .json(payload);
    send_json(req).await
}

pub async fn delete_memory_contact(contact_id: &str) -> Result<bool, String> {
    let req = client()
        .delete(try_build_url(&format!(
            "/contacts/{}",
            urlencoding::encode(contact_id)
        ))?)
        .timeout(try_timeout_duration()?);

    send_delete_result(req).await
}

pub async fn sync_memory_project(
    payload: &SyncMemoryProjectRequestDto,
) -> Result<MemoryProjectDto, String> {
    let req = client()
        .post(try_build_url("/projects/sync")?)
        .timeout(try_timeout_duration()?)
        .json(payload);
    send_json(req).await
}

pub async fn sync_project_agent_link(
    payload: &SyncProjectAgentLinkRequestDto,
) -> Result<MemoryProjectAgentLinkDto, String> {
    let req = client()
        .post(try_build_url("/project-agent-links/sync")?)
        .timeout(try_timeout_duration()?)
        .json(payload);
    send_json(req).await
}

pub async fn list_project_contacts(
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectContactDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    push_limit_offset_params(&mut params, limit, offset);

    let path = format!("/projects/{}/contacts", urlencoding::encode(project_id));
    send_list(path.as_str(), &params).await
}

pub async fn list_contact_project_memories(
    contact_id: &str,
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    push_limit_offset_params(&mut params, limit, offset);

    let path = format!(
        "/contacts/{}/project-memories/{}",
        urlencoding::encode(contact_id),
        urlencoding::encode(project_id)
    );
    send_list(path.as_str(), &params).await
}

pub async fn list_contact_project_memories_by_contact(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    push_limit_offset_params(&mut params, limit, offset);

    let path = format!(
        "/contacts/{}/project-memories",
        urlencoding::encode(contact_id)
    );
    send_list(path.as_str(), &params).await
}

pub async fn list_contact_projects(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<Value>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    push_limit_offset_params(&mut params, limit, offset);

    let path = format!("/contacts/{}/projects", urlencoding::encode(contact_id));
    send_list(path.as_str(), &params).await
}

pub async fn list_contact_agent_recalls(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryAgentRecallDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    push_limit_offset_params(&mut params, limit, offset);

    let path = format!(
        "/contacts/{}/agent-recalls",
        urlencoding::encode(contact_id)
    );
    send_list(path.as_str(), &params).await
}
