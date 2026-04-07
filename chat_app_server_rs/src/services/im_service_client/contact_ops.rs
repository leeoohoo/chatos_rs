use super::dto::{CreateImContactRequestDto, ImContactDto};
use super::http::{build_url, client, send_json, timeout_duration};

pub async fn list_contacts() -> Result<Vec<ImContactDto>, String> {
    let req = client()
        .get(build_url("/contacts").as_str())
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn create_contact(req_body: &CreateImContactRequestDto) -> Result<ImContactDto, String> {
    let req = client()
        .post(build_url("/contacts").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn get_contact(contact_id: &str) -> Result<ImContactDto, String> {
    let req = client()
        .get(build_url(&format!("/contacts/{}", urlencoding::encode(contact_id))).as_str())
        .timeout(timeout_duration());
    send_json(req).await
}
