use super::*;

pub(in crate::mcp_server) fn decode_args<T>(args: Value) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(args).map_err(|err| err.to_string())
}

pub(in crate::mcp_server) fn decode_remote_server_config_header(
    value: &str,
) -> Result<CreateRemoteServerRequest, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("远程服务器透传配置为空".to_string());
    }
    let json_text = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        let bytes = URL_SAFE_NO_PAD
            .decode(trimmed.as_bytes())
            .map_err(|err| format!("远程服务器透传配置不是有效 base64: {err}"))?;
        String::from_utf8(bytes).map_err(|err| format!("远程服务器透传配置不是 UTF-8: {err}"))?
    };
    serde_json::from_str::<CreateRemoteServerRequest>(&json_text)
        .map_err(|err| format!("远程服务器透传配置不是有效 JSON: {err}"))
}

pub(in crate::mcp_server) fn text_result(payload: Value) -> Value {
    let text = if payload.is_string() {
        payload.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
    };
    let mut out = json!({
        "content": [
            { "type": "text", "text": text }
        ]
    });
    if !payload.is_string() && !payload.is_null() {
        out["_structured_result"] = payload;
    }
    out
}
