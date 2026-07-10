// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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

pub const SANDBOX_IMAGE_PROJECT_ID_HEADER: &str = "x-chatos-sandbox-project-id";
pub const SANDBOX_IMAGE_RUN_ID_HEADER: &str = "x-chatos-sandbox-run-id";

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
                            "maxLength": 131072,
                            "description": "Optional non-interactive Bash script written by the AI. It runs as root during the Docker image build after requested runtime features are installed. A non-zero exit fails the image build. Do not place secrets in the script or its output."
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
                json!({ "images": images }),
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
    let custom_build_script = normalize_optional(args.custom_build_script);
    let mut requested_features = args.features.clone();
    if let Some(script) = custom_build_script.as_deref() {
        requested_features.push(custom_build_script_feature(script));
    }
    let search_args = SearchImagesArgs {
        features: requested_features,
        include_unavailable: false,
        ..SearchImagesArgs::default()
    };
    let catalog = backend.image_catalog().await?;
    let matches = search_images(&catalog, &search_args);
    if let Some(image) = matches.into_iter().find(image_is_available) {
        return Ok(ready_image_result(true, Some(image), None));
    }

    let job = backend
        .initialize_image(args.features, custom_build_script)
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
    Ok(ready_image_result(false, image, Some(final_job)))
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

fn ready_image_result(reused: bool, image: Option<Value>, job: Option<Value>) -> Value {
    let image = image.as_ref().map(compact_image);
    let job = job.as_ref().map(compact_job);
    let image_id = image
        .as_ref()
        .and_then(|value| value.get("id"))
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            job.as_ref()
                .and_then(|value| value.get("image_id"))
                .filter(|value| !value.is_null())
                .cloned()
        })
        .unwrap_or(Value::Null);
    let image_ref = image
        .as_ref()
        .and_then(|value| value.get("image_ref"))
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            job.as_ref()
                .and_then(|value| value.get("image_ref"))
                .filter(|value| !value.is_null())
                .cloned()
        })
        .unwrap_or(Value::Null);
    let status = image
        .as_ref()
        .and_then(|value| value.get("status"))
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            job.as_ref()
                .and_then(|value| value.get("status"))
                .filter(|value| !value.is_null())
                .cloned()
        })
        .unwrap_or(Value::Null);
    let features = image
        .as_ref()
        .and_then(|value| value.get("features"))
        .cloned()
        .unwrap_or_else(|| json!([]));

    json!({
        "reused": reused,
        "image_id": image_id,
        "image_ref": image_ref,
        "status": status,
        "features": features,
        "image": image,
        "job": job,
    })
}

fn compact_image(image: &Value) -> Value {
    json!({
        "id": image.get("id").cloned().unwrap_or(Value::Null),
        "image_ref": image.get("image_ref").cloned().unwrap_or(Value::Null),
        "status": image.get("status").cloned().unwrap_or(Value::Null),
        "initialized": image.get("initialized").cloned().unwrap_or(Value::Null),
        "features": image.get("features").cloned().unwrap_or_else(|| json!([])),
    })
}

fn compact_job(job: &Value) -> Value {
    json!({
        "id": job.get("id").cloned().unwrap_or(Value::Null),
        "image_id": job.get("image_id").cloned().unwrap_or(Value::Null),
        "image_ref": job.get("image_ref").cloned().unwrap_or(Value::Null),
        "status": job.get("status").cloned().unwrap_or(Value::Null),
        "error": job.get("error").cloned().unwrap_or(Value::Null),
    })
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

pub fn custom_build_script_hash(script: &str) -> String {
    let digest = Sha256::digest(script.as_bytes());
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn custom_build_script_feature(script: &str) -> String {
    format!("script@{}", custom_build_script_hash(script))
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct TestBackend {
        catalog: Value,
        initialize_calls: AtomicUsize,
    }

    #[async_trait]
    impl SandboxImageBackend for TestBackend {
        async fn image_catalog(&self) -> Result<Value, String> {
            Ok(self.catalog.clone())
        }

        async fn image_jobs(&self) -> Result<Value, String> {
            Ok(json!([]))
        }

        async fn initialize_image(
            &self,
            _features: Vec<String>,
            _custom_build_script: Option<String>,
        ) -> Result<Value, String> {
            self.initialize_calls.fetch_add(1, Ordering::SeqCst);
            Ok(json!({
                "id": "job-1",
                "image_id": "custom-image",
                "status": "succeeded"
            }))
        }
    }

    fn ready_image(features: Vec<String>) -> Value {
        json!({
            "id": "image-1",
            "features": features,
            "initialized": true,
            "status": "ready"
        })
    }

    #[tokio::test]
    async fn custom_script_does_not_reuse_runtime_only_image() {
        let backend = TestBackend {
            catalog: json!({ "images": [ready_image(vec!["node@24".to_string()])] }),
            initialize_calls: AtomicUsize::new(0),
        };

        ensure_image(
            &backend,
            CreateImageArgs {
                features: vec!["node@24".to_string()],
                custom_build_script: Some(
                    "apt-get update && apt-get install -y ffmpeg".to_string(),
                ),
                ..CreateImageArgs::default()
            },
        )
        .await
        .expect("custom image build should start");

        assert_eq!(backend.initialize_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn custom_script_reuses_only_matching_script_image() {
        let script = "apt-get update && apt-get install -y ffmpeg";
        let backend = TestBackend {
            catalog: json!({
                "images": [ready_image(vec![
                    "node@24".to_string(),
                    custom_build_script_feature(script),
                ])]
            }),
            initialize_calls: AtomicUsize::new(0),
        };

        let result = ensure_image(
            &backend,
            CreateImageArgs {
                features: vec!["node@24".to_string()],
                custom_build_script: Some(script.to_string()),
                ..CreateImageArgs::default()
            },
        )
        .await
        .expect("matching custom image should be reused");

        assert_eq!(result.get("reused").and_then(Value::as_bool), Some(true));
        assert_eq!(backend.initialize_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ready_image_result_keeps_identity_and_omits_large_job_output() {
        let result = ready_image_result(
            false,
            None,
            Some(json!({
                "id": "job-1",
                "image_id": "custom-image",
                "image_ref": "chatos/sandbox:custom-image",
                "status": "succeeded",
                "output": "x".repeat(20_000),
            })),
        );

        assert_eq!(
            result.get("image_id").and_then(Value::as_str),
            Some("custom-image")
        );
        assert_eq!(
            result.get("image_ref").and_then(Value::as_str),
            Some("chatos/sandbox:custom-image")
        );
        assert!(!result.to_string().contains(&"x".repeat(1_000)));
        assert!(result.to_string().chars().count() < 8_000);
    }
}
