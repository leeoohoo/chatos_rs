// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::time::{sleep, Instant};
use uuid::Uuid;

const TOOL_GET_IMAGE_CATALOG: &str = "get_image_catalog";
const TOOL_SEARCH_IMAGES: &str = "search_images";
const TOOL_CREATE_IMAGE: &str = "create_image";
const DEFAULT_CREATE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_CREATE_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const MIN_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);

#[async_trait]
pub trait SandboxImageBackend: Send + Sync {
    async fn image_catalog(&self) -> Result<Value, String>;
    async fn image_jobs(&self) -> Result<Value, String>;
    async fn initialize_image(
        &self,
        features: Vec<String>,
        custom_build_script: Option<String>,
    ) -> Result<Value, String>;
}

#[derive(Debug, Clone, Deserialize)]
struct ToolCallRequest {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SearchImagesArgs {
    #[serde(default)]
    image_id: Option<String>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    include_unavailable: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CreateImageArgs {
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    custom_build_script: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct ToolTextResult {
    content: Vec<ToolTextContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    structured_content: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct ToolTextContent {
    r#type: String,
    text: String,
}

pub fn list_tools() -> Value {
    json!({
        "tools": [
            {
                "name": TOOL_GET_IMAGE_CATALOG,
                "description": "View the sandbox image catalog, including supported runtime features and known images.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": TOOL_SEARCH_IMAGES,
                "description": "Search existing sandbox images by image id, runtime features, and availability status.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "image_id": {"type": "string"},
                        "features": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Runtime features such as node@24, java@21, redis, mysql."
                        },
                        "status": {"type": "string"},
                        "include_unavailable": {"type": "boolean"}
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": TOOL_CREATE_IMAGE,
                "description": "Synchronously create or reuse a sandbox image for the requested runtime features. This tool waits until image creation succeeds or fails.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "features": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Runtime features to install in the image."
                        },
                        "custom_build_script": {
                            "type": "string",
                            "description": "Optional build script appended by the sandbox image builder."
                        },
                        "timeout_ms": {
                            "type": "integer",
                            "description": "Optional synchronous wait timeout. Defaults to 30 minutes and is capped at 2 hours."
                        },
                        "poll_interval_ms": {
                            "type": "integer",
                            "description": "Optional job polling interval. Defaults to 5 seconds."
                        }
                    },
                    "additionalProperties": false
                }
            }
        ]
    })
}

pub async fn handle_jsonrpc<B>(backend: &B, payload: Value) -> Value
where
    B: SandboxImageBackend,
{
    let id = payload.get("id").cloned().unwrap_or(Value::Null);
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let result = match method {
        "tools/list" => Ok(list_tools()),
        "tools/call" => {
            let params = payload.get("params").cloned().unwrap_or_else(|| json!({}));
            call_tool(backend, params).await
        }
        _ => Err(format!("unsupported JSON-RPC method: {method}")),
    };
    match result {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
        Err(err) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": err,
            },
        }),
    }
}

pub async fn call_tool<B>(backend: &B, params: Value) -> Result<Value, String>
where
    B: SandboxImageBackend,
{
    let request: ToolCallRequest =
        serde_json::from_value(params).map_err(|err| format!("invalid tool call params: {err}"))?;
    match request.name.as_str() {
        TOOL_GET_IMAGE_CATALOG => {
            let catalog = backend.image_catalog().await?;
            Ok(tool_result("Sandbox image catalog loaded.", catalog))
        }
        TOOL_SEARCH_IMAGES => {
            let args = parse_args::<SearchImagesArgs>(request.arguments)?;
            let catalog = backend.image_catalog().await?;
            let images = search_images(&catalog, &args);
            Ok(tool_result(
                format!("Found {} matching sandbox image(s).", images.len()),
                json!({
                    "images": images,
                    "catalog": catalog,
                }),
            ))
        }
        TOOL_CREATE_IMAGE => {
            let args = parse_args::<CreateImageArgs>(request.arguments)?;
            let created = ensure_image(backend, args).await?;
            Ok(tool_result("Sandbox image is ready.", created))
        }
        other => Err(format!("unknown sandbox image tool: {other}")),
    }
}

async fn ensure_image<B>(backend: &B, args: CreateImageArgs) -> Result<Value, String>
where
    B: SandboxImageBackend,
{
    let search_args = SearchImagesArgs {
        features: args.features.clone(),
        include_unavailable: false,
        ..SearchImagesArgs::default()
    };
    let catalog = backend.image_catalog().await?;
    let matches = search_images(&catalog, &search_args);
    if let Some(image) = matches.into_iter().find(image_is_available) {
        return Ok(json!({
            "reused": true,
            "image": image,
            "job": null,
        }));
    }

    let job = backend
        .initialize_image(args.features, normalize_optional(args.custom_build_script))
        .await?;
    let timeout = timeout_from_ms(args.timeout_ms);
    let poll_interval = poll_interval_from_ms(args.poll_interval_ms);
    let final_job = wait_for_job(backend, &job, timeout, poll_interval).await?;
    if job_status(&final_job) == Some("failed") {
        let reason = final_job
            .get("error")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| final_job.get("output").and_then(Value::as_str))
            .unwrap_or("sandbox image creation failed");
        return Err(reason.to_string());
    }
    let refreshed = backend.image_catalog().await?;
    let image = find_image_by_job(&refreshed, &final_job).or_else(|| {
        search_images(&refreshed, &search_args)
            .into_iter()
            .find(image_is_available)
    });
    Ok(json!({
        "reused": false,
        "image": image,
        "job": final_job,
        "catalog": refreshed,
    }))
}

async fn wait_for_job<B>(
    backend: &B,
    initial_job: &Value,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<Value, String>
where
    B: SandboxImageBackend,
{
    let job_id = normalized_value(initial_job.get("id").and_then(Value::as_str))
        .ok_or_else(|| "sandbox image initialize did not return a job id".to_string())?;
    if job_is_terminal(initial_job) {
        return Ok(initial_job.clone());
    }

    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            return Err(format!(
                "sandbox image creation timed out after {} ms",
                timeout.as_millis()
            ));
        }
        sleep(poll_interval).await;
        let jobs = backend.image_jobs().await?;
        if let Some(job) = find_job(&jobs, job_id.as_str()) {
            if job_is_terminal(&job) {
                return Ok(job);
            }
        }
    }
}

fn find_job(jobs: &Value, job_id: &str) -> Option<Value> {
    jobs.as_array()
        .into_iter()
        .flatten()
        .find(|job| job.get("id").and_then(Value::as_str) == Some(job_id))
        .cloned()
}

fn find_image_by_job(catalog: &Value, job: &Value) -> Option<Value> {
    let image_id = job.get("image_id").and_then(Value::as_str)?;
    catalog_images(catalog)
        .into_iter()
        .find(|image| image.get("id").and_then(Value::as_str) == Some(image_id))
}

fn search_images(catalog: &Value, args: &SearchImagesArgs) -> Vec<Value> {
    let requested_features = normalize_feature_set(args.features.as_slice());
    let image_id = normalized_value(args.image_id.as_deref());
    let status = normalized_value(args.status.as_deref()).map(|value| value.to_ascii_lowercase());
    catalog_images(catalog)
        .into_iter()
        .filter(|image| {
            if let Some(image_id) = image_id.as_deref() {
                if image.get("id").and_then(Value::as_str) != Some(image_id) {
                    return false;
                }
            }
            if !requested_features.is_empty()
                && !image_feature_set(image).is_superset(&requested_features)
            {
                return false;
            }
            if let Some(status) = status.as_deref() {
                if image_status(image)
                    .map(|value| value.to_ascii_lowercase())
                    .as_deref()
                    != Some(status)
                {
                    return false;
                }
            }
            args.include_unavailable || image_is_available(image)
        })
        .collect()
}

fn catalog_images(catalog: &Value) -> Vec<Value> {
    catalog
        .get("images")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn image_feature_set(image: &Value) -> HashSet<String> {
    image
        .get("features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(normalize_feature)
        .collect()
}

fn normalize_feature_set(features: &[String]) -> HashSet<String> {
    features
        .iter()
        .filter_map(|item| normalize_feature(item.as_str()))
        .collect()
}

fn normalize_feature(value: &str) -> Option<String> {
    normalized_value(Some(value)).map(|value| value.to_ascii_lowercase())
}

fn image_is_available(image: &Value) -> bool {
    image
        .get("initialized")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || matches!(
            image_status(image)
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "local" | "ready" | "succeeded" | "initialized"
        )
}

fn image_status(image: &Value) -> Option<String> {
    normalized_value(image.get("status").and_then(Value::as_str))
}

fn job_is_terminal(job: &Value) -> bool {
    matches!(job_status(job), Some("succeeded" | "failed"))
}

fn job_status(job: &Value) -> Option<&str> {
    job.get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn parse_args<T>(value: Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value).map_err(|err| format!("invalid sandbox image tool args: {err}"))
}

fn tool_result(message: impl Into<String>, structured: Value) -> Value {
    let message = message.into();
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| message.clone());
    serde_json::to_value(ToolTextResult {
        content: vec![ToolTextContent {
            r#type: "text".to_string(),
            text: format!("{message}\n{text}"),
        }],
        structured_content: Some(structured),
    })
    .unwrap_or_else(|_| json!({"content": [{"type": "text", "text": message}]}))
}

fn timeout_from_ms(value: Option<u64>) -> Duration {
    value
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_CREATE_TIMEOUT)
        .clamp(Duration::from_millis(1), MAX_CREATE_TIMEOUT)
}

fn poll_interval_from_ms(value: Option<u64>) -> Duration {
    value
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_POLL_INTERVAL)
        .clamp(MIN_POLL_INTERVAL, MAX_POLL_INTERVAL)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| normalized_value(Some(value.as_str())))
}

fn normalized_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn server_name(prefix: &str) -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    format!("{prefix}_{suffix}")
}
