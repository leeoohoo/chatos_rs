use axum::body::Body;
use axum::http::{
    header::{self, HeaderValue},
    StatusCode,
};
use axum::response::Response;
use serde_json::json;

pub fn json_error_response(status: StatusCode, message: impl AsRef<str>) -> Response {
    let body = json!({ "error": message.as_ref() });
    let bytes =
        serde_json::to_vec(&body).unwrap_or_else(|_| b"{\"error\":\"internal_error\"}".to_vec());
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

pub fn binary_download_response(data: Vec<u8>, content_type: &str, file_name: &str) -> Response {
    let mut response = Response::new(Body::from(data));
    *response.status_mut() = StatusCode::OK;

    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    } else {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
    }

    let disposition = build_content_disposition(file_name);
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }

    response
}

fn build_content_disposition(file_name: &str) -> String {
    let fallback = sanitize_ascii_filename(file_name);
    let encoded = urlencoding::encode(file_name);
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

fn sanitize_ascii_filename(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('.').trim_matches('_').to_string();
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed
    }
}
