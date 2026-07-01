// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::Arc;

use base64::engine::general_purpose;
use base64::Engine as _;
use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::{AppConfig, SandboxBackendKind};
use crate::models::{
    SandboxImageCatalogResponse, SandboxImageFeatureRecord, SandboxImageJobRecord,
    SandboxImageRecord, SandboxImageRuntimeVersionRecord,
};

const DEFAULT_IMAGE_ID: &str = "default";
const JOB_STATUS_RUNNING: &str = "running";
const JOB_STATUS_SUCCEEDED: &str = "succeeded";
const JOB_STATUS_FAILED: &str = "failed";
const MAX_JOB_OUTPUT_LEN: usize = 80_000;
const MAX_CUSTOM_BUILD_SCRIPT_LEN: usize = 128 * 1024;
const DEFAULT_IMAGE_FEATURES: [&str; 4] = ["java@21", "node@24", "rust@stable", "go@1.26"];

#[derive(Debug, Clone, Default)]
pub(crate) struct ImageJobStore {
    jobs: Arc<RwLock<HashMap<String, SandboxImageJobRecord>>>,
}

impl ImageJobStore {
    pub(crate) async fn insert(&self, job: SandboxImageJobRecord) {
        self.jobs.write().await.insert(job.id.clone(), job);
    }

    pub(crate) async fn list(&self) -> Vec<SandboxImageJobRecord> {
        let mut jobs = self.jobs.read().await.values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        jobs
    }

    async fn active_for_image(&self, image_id: &str) -> Option<SandboxImageJobRecord> {
        self.jobs
            .read()
            .await
            .values()
            .find(|job| job.image_id == image_id && job.status == JOB_STATUS_RUNNING)
            .cloned()
    }

    async fn update<F>(&self, job_id: &str, update: F)
    where
        F: FnOnce(&mut SandboxImageJobRecord),
    {
        if let Some(job) = self.jobs.write().await.get_mut(job_id) {
            update(job);
            job.updated_at = now_rfc3339();
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RuntimeVersionSpec {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    default: bool,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeSpec {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    aliases: &'static [&'static str],
    versions: &'static [RuntimeVersionSpec],
}

#[derive(Debug, Clone, Copy)]
struct RuntimeSelectionSpec {
    runtime: RuntimeSpec,
    version: RuntimeVersionSpec,
}

#[derive(Debug, Clone)]
struct ImageBuildSpec {
    record: SandboxImageRecord,
    install_features: Vec<String>,
    custom_build_script: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedImageId {
    selections: Vec<RuntimeSelectionSpec>,
    custom_script_hash: Option<String>,
}

const JAVA_VERSIONS: [RuntimeVersionSpec; 5] = [
    RuntimeVersionSpec {
        id: "8",
        label: "JDK 8",
        description: "Temurin JDK 8 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "11",
        label: "JDK 11",
        description: "Temurin JDK 11 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "17",
        label: "JDK 17",
        description: "Temurin JDK 17 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "21",
        label: "JDK 21",
        description: "Temurin JDK 21 LTS",
        default: true,
    },
    RuntimeVersionSpec {
        id: "25",
        label: "JDK 25",
        description: "Temurin JDK 25 LTS",
        default: false,
    },
];

const NODE_VERSIONS: [RuntimeVersionSpec; 4] = [
    RuntimeVersionSpec {
        id: "20",
        label: "Node.js 20",
        description: "Node.js 20 legacy line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "22",
        label: "Node.js 22",
        description: "Node.js 22 LTS",
        default: false,
    },
    RuntimeVersionSpec {
        id: "24",
        label: "Node.js 24",
        description: "Node.js 24 LTS",
        default: true,
    },
    RuntimeVersionSpec {
        id: "26",
        label: "Node.js 26",
        description: "Node.js 26 current line",
        default: false,
    },
];

const PYTHON_VERSIONS: [RuntimeVersionSpec; 5] = [
    RuntimeVersionSpec {
        id: "3.10",
        label: "Python 3.10",
        description: "Python 3.10 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.11",
        label: "Python 3.11",
        description: "Python 3.11 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.12",
        label: "Python 3.12",
        description: "Python 3.12 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.13",
        label: "Python 3.13",
        description: "Python 3.13 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.14",
        label: "Python 3.14",
        description: "Python 3.14 active support line",
        default: true,
    },
];

const RUST_VERSIONS: [RuntimeVersionSpec; 7] = [
    RuntimeVersionSpec {
        id: "1.85.1",
        label: "Rust 1.85.1",
        description: "Pinned Rust 1.85.1 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.88.0",
        label: "Rust 1.88.0",
        description: "Pinned Rust 1.88.0 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.92.0",
        label: "Rust 1.92.0",
        description: "Pinned Rust 1.92.0 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.96.1",
        label: "Rust 1.96.1",
        description: "Pinned Rust 1.96.1 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "stable",
        label: "Stable",
        description: "Rust stable channel",
        default: true,
    },
    RuntimeVersionSpec {
        id: "beta",
        label: "Beta",
        description: "Rust beta channel",
        default: false,
    },
    RuntimeVersionSpec {
        id: "nightly",
        label: "Nightly",
        description: "Rust nightly channel",
        default: false,
    },
];

const GO_VERSIONS: [RuntimeVersionSpec; 5] = [
    RuntimeVersionSpec {
        id: "1.22",
        label: "Go 1.22",
        description: "Go 1.22 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.23",
        label: "Go 1.23",
        description: "Go 1.23 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.24",
        label: "Go 1.24",
        description: "Go 1.24 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.25",
        label: "Go 1.25",
        description: "Go 1.25 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "1.26",
        label: "Go 1.26",
        description: "Go 1.26 toolchain",
        default: true,
    },
];

const DOTNET_VERSIONS: [RuntimeVersionSpec; 3] = [
    RuntimeVersionSpec {
        id: "8.0",
        label: ".NET 8",
        description: ".NET 8 LTS SDK",
        default: false,
    },
    RuntimeVersionSpec {
        id: "9.0",
        label: ".NET 9",
        description: ".NET 9 STS SDK",
        default: false,
    },
    RuntimeVersionSpec {
        id: "10.0",
        label: ".NET 10",
        description: ".NET 10 LTS SDK",
        default: true,
    },
];

const PHP_VERSIONS: [RuntimeVersionSpec; 4] = [
    RuntimeVersionSpec {
        id: "8.2",
        label: "PHP 8.2",
        description: "PHP 8.2 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "8.3",
        label: "PHP 8.3",
        description: "PHP 8.3 security support line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "8.4",
        label: "PHP 8.4",
        description: "PHP 8.4 active support line",
        default: true,
    },
    RuntimeVersionSpec {
        id: "8.5",
        label: "PHP 8.5",
        description: "PHP 8.5 active support line",
        default: false,
    },
];

const RUBY_VERSIONS: [RuntimeVersionSpec; 4] = [
    RuntimeVersionSpec {
        id: "3.2.11",
        label: "Ruby 3.2.11",
        description: "Ruby 3.2 maintenance line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.3.11",
        label: "Ruby 3.3.11",
        description: "Ruby 3.3 maintenance line",
        default: false,
    },
    RuntimeVersionSpec {
        id: "3.4.10",
        label: "Ruby 3.4.10",
        description: "Ruby 3.4 stable line",
        default: true,
    },
    RuntimeVersionSpec {
        id: "4.0.5",
        label: "Ruby 4.0.5",
        description: "Ruby 4.0 current line",
        default: false,
    },
];

const GCC_VERSIONS: [RuntimeVersionSpec; 2] = [
    RuntimeVersionSpec {
        id: "13",
        label: "GCC 13",
        description: "GNU C/C++ compiler 13",
        default: false,
    },
    RuntimeVersionSpec {
        id: "14",
        label: "GCC 14",
        description: "GNU C/C++ compiler 14",
        default: true,
    },
];

const CLANG_VERSIONS: [RuntimeVersionSpec; 3] = [
    RuntimeVersionSpec {
        id: "18",
        label: "Clang 18",
        description: "LLVM/Clang 18 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "19",
        label: "Clang 19",
        description: "LLVM/Clang 19 toolchain",
        default: false,
    },
    RuntimeVersionSpec {
        id: "20",
        label: "Clang 20",
        description: "LLVM/Clang 20 toolchain",
        default: true,
    },
];

const IMAGE_RUNTIMES: [RuntimeSpec; 10] = [
    RuntimeSpec {
        id: "java",
        label: "JDK",
        description: "OpenJDK development tools",
        aliases: &["jdk", "openjdk"],
        versions: &JAVA_VERSIONS,
    },
    RuntimeSpec {
        id: "node",
        label: "Node.js",
        description: "Node.js, npm, pnpm and yarn",
        aliases: &["js", "nodejs", "javascript", "typescript"],
        versions: &NODE_VERSIONS,
    },
    RuntimeSpec {
        id: "python",
        label: "Python",
        description: "Python interpreter, pip and venv tooling",
        aliases: &["py", "python3"],
        versions: &PYTHON_VERSIONS,
    },
    RuntimeSpec {
        id: "rust",
        label: "Rust",
        description: "Rust toolchain",
        aliases: &["cargo"],
        versions: &RUST_VERSIONS,
    },
    RuntimeSpec {
        id: "go",
        label: "Go",
        description: "Go toolchain",
        aliases: &["golang"],
        versions: &GO_VERSIONS,
    },
    RuntimeSpec {
        id: "dotnet",
        label: ".NET",
        description: ".NET SDK for C# and F# projects",
        aliases: &["csharp", "cs", "fsharp"],
        versions: &DOTNET_VERSIONS,
    },
    RuntimeSpec {
        id: "php",
        label: "PHP",
        description: "PHP CLI runtime and Composer",
        aliases: &[],
        versions: &PHP_VERSIONS,
    },
    RuntimeSpec {
        id: "ruby",
        label: "Ruby",
        description: "Ruby runtime, RubyGems and Bundler",
        aliases: &["rails"],
        versions: &RUBY_VERSIONS,
    },
    RuntimeSpec {
        id: "gcc",
        label: "C/C++ (GCC)",
        description: "GNU C and C++ compiler toolchain",
        aliases: &["c", "cpp", "c++", "cplusplus", "g++"],
        versions: &GCC_VERSIONS,
    },
    RuntimeSpec {
        id: "clang",
        label: "C/C++ (Clang)",
        description: "LLVM, Clang, LLD and Clangd toolchain",
        aliases: &["llvm"],
        versions: &CLANG_VERSIONS,
    },
];

pub(crate) async fn catalog(
    config: &AppConfig,
    backend: SandboxBackendKind,
) -> SandboxImageCatalogResponse {
    let local_refs = local_image_refs(config, backend).await;
    let mut images = Vec::new();
    let mut default_record = default_image_record(config, backend);
    apply_catalog_status(backend, &local_refs, &mut default_record);
    images.push(default_record);

    if let Ok(refs) = &local_refs {
        let mut local_images = refs
            .iter()
            .filter_map(|image_ref| local_image_record(config, backend, image_ref))
            .collect::<Vec<_>>();
        local_images.sort_by(|left, right| left.name.cmp(&right.name));
        images.extend(local_images);
    }

    SandboxImageCatalogResponse {
        backend: backend.as_str().to_string(),
        default_image_id: DEFAULT_IMAGE_ID.to_string(),
        image_tag_prefix: normalized_tag_prefix(config),
        features: IMAGE_RUNTIMES
            .into_iter()
            .map(|runtime| SandboxImageFeatureRecord {
                id: runtime.id.to_string(),
                label: runtime.label.to_string(),
                description: runtime.description.to_string(),
                default_version: default_version(runtime).id.to_string(),
                versions: runtime
                    .versions
                    .iter()
                    .map(|version| SandboxImageRuntimeVersionRecord {
                        id: version.id.to_string(),
                        label: version.label.to_string(),
                        description: version.description.to_string(),
                        default: version.default,
                    })
                    .collect(),
            })
            .collect(),
        images,
    }
}

pub(crate) async fn start_initialize_job(
    jobs: ImageJobStore,
    config: &AppConfig,
    backend: SandboxBackendKind,
    features: &[String],
    custom_build_script: Option<&str>,
) -> Result<SandboxImageJobRecord, String> {
    let feature_specs = canonical_features(features)?;
    let custom_build_script = normalize_custom_build_script(custom_build_script)?;
    let custom_script_hash = custom_build_script.as_deref().map(custom_build_script_hash);
    let image = generated_image_record(
        config,
        backend,
        &feature_specs,
        custom_script_hash.as_deref(),
    );
    let install_features = feature_specs
        .iter()
        .map(selection_feature_token)
        .collect::<Vec<_>>();

    if let Some(job) = jobs.active_for_image(image.id.as_str()).await {
        return Ok(job);
    }

    let now = now_rfc3339();
    let job = SandboxImageJobRecord {
        id: format!("image-job-{}", Uuid::new_v4()),
        image_id: image.id.clone(),
        image_name: image.name.clone(),
        image_ref: image.image_ref.clone(),
        features: image.features.clone(),
        backend: backend.as_str().to_string(),
        status: JOB_STATUS_RUNNING.to_string(),
        created_at: now.clone(),
        updated_at: now,
        started_at: Some(now_rfc3339()),
        finished_at: None,
        output: String::new(),
        error: None,
    };
    jobs.insert(job.clone()).await;

    let job_id = job.id.clone();
    let config = config.clone();
    let build = ImageBuildSpec {
        record: image,
        install_features,
        custom_build_script,
    };
    tokio::spawn(async move {
        run_initialize_job(jobs, config, backend, job_id, build).await;
    });

    Ok(job)
}

async fn run_initialize_job(
    jobs: ImageJobStore,
    config: AppConfig,
    backend: SandboxBackendKind,
    job_id: String,
    build: ImageBuildSpec,
) {
    if matches!(backend, SandboxBackendKind::Mock) {
        jobs.update(job_id.as_str(), |job| {
            job.status = JOB_STATUS_SUCCEEDED.to_string();
            job.finished_at = Some(now_rfc3339());
            append_job_output(job, "mock backend does not build container images\n");
        })
        .await;
        return;
    }

    let cli = container_cli(&config, backend).to_string();
    jobs.update(job_id.as_str(), |job| {
        append_job_output(
            job,
            &format!("starting image build: {}\n", build.record.image_ref),
        );
        if build.custom_build_script.is_some() {
            append_job_output(job, "custom build script is enabled\n");
        }
    })
    .await;

    let custom_script_b64 = build
        .custom_build_script
        .as_deref()
        .map(|script| general_purpose::STANDARD.encode(script.as_bytes()));
    let mut command = Command::new(&cli);
    command
        .arg("build")
        .arg("-t")
        .arg(&build.record.image_ref)
        .arg("-f")
        .arg(&config.image_dockerfile)
        .arg("--build-arg")
        .arg(format!(
            "SANDBOX_FEATURES={}",
            build.install_features.join(",")
        ));
    if let Some(custom_script_b64) = &custom_script_b64 {
        command
            .arg("--build-arg")
            .arg(format!("SANDBOX_CUSTOM_SCRIPT_B64={custom_script_b64}"));
    }
    command
        .arg(&config.image_build_context)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(format!("start image build failed with {cli}: {err}"));
                append_job_output(
                    job,
                    &format!("start image build failed with {cli}: {err}\n"),
                );
            })
            .await;
            return;
        }
    };

    let stdout_reader = child
        .stdout
        .take()
        .map(|stdout| tokio::spawn(read_job_output(jobs.clone(), job_id.clone(), stdout)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_job_output(jobs.clone(), job_id.clone(), stderr)));

    let status = child.wait().await;
    if let Some(reader) = stdout_reader {
        let _ = reader.await;
    }
    if let Some(reader) = stderr_reader {
        let _ = reader.await;
    }

    match status {
        Ok(status) if status.success() => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_SUCCEEDED.to_string();
                job.finished_at = Some(now_rfc3339());
                append_job_output(job, "image build completed\n");
            })
            .await;
        }
        Ok(status) => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(format!("image build exited with {status}"));
                append_job_output(job, &format!("image build exited with {status}\n"));
            })
            .await;
        }
        Err(err) => {
            jobs.update(job_id.as_str(), |job| {
                job.status = JOB_STATUS_FAILED.to_string();
                job.finished_at = Some(now_rfc3339());
                job.error = Some(format!("wait image build failed: {err}"));
                append_job_output(job, &format!("wait image build failed: {err}\n"));
            })
            .await;
        }
    }
}

async fn read_job_output<R>(jobs: ImageJobStore, job_id: String, stream: R)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(stream).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                jobs.update(job_id.as_str(), |job| {
                    append_job_output(job, &line);
                    append_job_output(job, "\n");
                })
                .await;
            }
            Ok(None) => break,
            Err(err) => {
                jobs.update(job_id.as_str(), |job| {
                    append_job_output(job, &format!("read image build output failed: {err}\n"));
                })
                .await;
                break;
            }
        }
    }
}

pub(crate) async fn resolve_for_create(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_id: Option<&str>,
) -> Result<SandboxImageRecord, String> {
    let image_id = image_id.map(str::trim).filter(|value| !value.is_empty());
    let Some(image_id) = image_id else {
        return Ok(default_image_record(config, backend));
    };
    if image_id == DEFAULT_IMAGE_ID {
        return Ok(default_image_record(config, backend));
    }

    let mut record = generated_image_record_for_id(config, backend, image_id)
        .ok_or_else(|| format!("unknown sandbox image id: {image_id}"))?;
    apply_status(config, backend, &mut record).await;
    if !record.initialized {
        return Err(format!(
            "sandbox image {} is not initialized; initialize it before creating a sandbox",
            record.name
        ));
    }
    Ok(record)
}

async fn apply_status(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image: &mut SandboxImageRecord,
) {
    if matches!(backend, SandboxBackendKind::Mock) {
        image.initialized = true;
        image.status = "mock".to_string();
        return;
    }

    match image_exists(config, backend, image.image_ref.as_str()).await {
        Ok(true) => {
            image.initialized = true;
            image.status = "ready".to_string();
        }
        Ok(false) => {
            image.initialized = false;
            image.status = "missing".to_string();
        }
        Err(err) => {
            image.initialized = false;
            image.status = format!("inspect_error: {err}");
        }
    }
}

fn apply_catalog_status(
    backend: SandboxBackendKind,
    local_refs: &Result<HashSet<String>, String>,
    image: &mut SandboxImageRecord,
) {
    if matches!(backend, SandboxBackendKind::Mock) {
        image.initialized = true;
        image.status = "mock".to_string();
        return;
    }

    match local_refs {
        Ok(refs) if refs.contains(image.image_ref.as_str()) => {
            image.initialized = true;
            image.status = "ready".to_string();
        }
        Ok(_) => {
            image.initialized = false;
            image.status = "missing".to_string();
        }
        Err(err) => {
            image.initialized = false;
            image.status = format!("inspect_error: {err}");
        }
    }
}

async fn local_image_refs(
    config: &AppConfig,
    backend: SandboxBackendKind,
) -> Result<HashSet<String>, String> {
    if matches!(backend, SandboxBackendKind::Mock) {
        return Ok(HashSet::new());
    }
    let cli = container_cli(config, backend);
    let output = Command::new(cli)
        .arg("image")
        .arg("ls")
        .arg("--format")
        .arg("{{.Repository}}:{{.Tag}}")
        .output()
        .await
        .map_err(|err| format!("{cli} image ls failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{cli} image ls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains("<none>"))
        .map(ToOwned::to_owned)
        .collect())
}

async fn image_exists(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_ref: &str,
) -> Result<bool, String> {
    let cli = container_cli(config, backend);
    let output = Command::new(cli)
        .arg("image")
        .arg("inspect")
        .arg(image_ref)
        .output()
        .await
        .map_err(|err| format!("{cli} image inspect failed: {err}"))?;
    Ok(output.status.success())
}

fn default_image_record(config: &AppConfig, backend: SandboxBackendKind) -> SandboxImageRecord {
    SandboxImageRecord {
        id: DEFAULT_IMAGE_ID.to_string(),
        name: "Default".to_string(),
        description: "Service default image from runtime configuration".to_string(),
        image_ref: default_image_ref(config, backend),
        features: DEFAULT_IMAGE_FEATURES
            .iter()
            .map(|feature| (*feature).to_string())
            .collect(),
        backend: backend.as_str().to_string(),
        initialized: false,
        status: "unknown".to_string(),
        buildable: false,
        is_default: true,
    }
}

fn generated_image_record(
    config: &AppConfig,
    backend: SandboxBackendKind,
    selections: &[RuntimeSelectionSpec],
    custom_script_hash: Option<&str>,
) -> SandboxImageRecord {
    let mut feature_ids = selections
        .iter()
        .map(selection_feature_token)
        .collect::<Vec<_>>();
    if let Some(hash) = custom_script_hash {
        feature_ids.push(format!("script@{hash}"));
    }
    let id = generated_image_id(&feature_ids, custom_script_hash);
    let name = if selections.is_empty() {
        if let Some(hash) = custom_script_hash {
            format!("Base + Custom script {hash}")
        } else {
            "Base".to_string()
        }
    } else {
        let mut names = selections
            .iter()
            .map(selection_label)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if let Some(hash) = custom_script_hash {
            names.push(format!("Custom script {hash}"));
        }
        names.join(" + ")
    };
    let description = if selections.is_empty() {
        if custom_script_hash.is_some() {
            "Base image with custom build script".to_string()
        } else {
            "Base image with common shell, git, Python and workspace tools".to_string()
        }
    } else {
        format!("Development image with {name}")
    };

    SandboxImageRecord {
        id: id.clone(),
        name,
        description,
        image_ref: format!("{}:{id}", normalized_tag_prefix(config)),
        features: feature_ids,
        backend: backend.as_str().to_string(),
        initialized: false,
        status: "unknown".to_string(),
        buildable: true,
        is_default: false,
    }
}

fn generated_image_record_for_id(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_id: &str,
) -> Option<SandboxImageRecord> {
    let parsed = parse_generated_image_id(image_id)?;
    let mut record = generated_image_record(
        config,
        backend,
        &parsed.selections,
        parsed.custom_script_hash.as_deref(),
    );
    if record.id != image_id {
        record.id = image_id.to_string();
        record.image_ref = format!("{}:{image_id}", normalized_tag_prefix(config));
    }
    Some(record)
}

fn local_image_record(
    config: &AppConfig,
    backend: SandboxBackendKind,
    image_ref: &str,
) -> Option<SandboxImageRecord> {
    let prefix = normalized_tag_prefix(config);
    let tag = image_ref.strip_prefix(format!("{prefix}:").as_str())?;
    let parsed = parse_generated_image_id(tag)?;
    let mut record = generated_image_record(
        config,
        backend,
        &parsed.selections,
        parsed.custom_script_hash.as_deref(),
    );
    record.id = tag.to_string();
    record.image_ref = image_ref.to_string();
    record.initialized = true;
    record.status = "ready".to_string();
    Some(record)
}

fn canonical_features(features: &[String]) -> Result<Vec<RuntimeSelectionSpec>, String> {
    let mut selections = Vec::new();
    for feature in features {
        let Some(selection) = parse_runtime_selection(feature)? else {
            continue;
        };
        if selections
            .iter()
            .any(|existing: &RuntimeSelectionSpec| existing.runtime.id == selection.runtime.id)
        {
            return Err(format!(
                "runtime {} is selected more than once",
                selection.runtime.label
            ));
        }
        selections.push(selection);
    }

    selections.sort_by_key(|selection| runtime_index(selection.runtime.id));
    Ok(selections)
}

fn normalize_custom_build_script(script: Option<&str>) -> Result<Option<String>, String> {
    let Some(script) = script else {
        return Ok(None);
    };
    let script = script.trim();
    if script.is_empty() {
        return Ok(None);
    }
    if script.len() > MAX_CUSTOM_BUILD_SCRIPT_LEN {
        return Err(format!(
            "custom build script is too large; maximum size is {} bytes",
            MAX_CUSTOM_BUILD_SCRIPT_LEN
        ));
    }
    if script.contains('\0') {
        return Err("custom build script cannot contain NUL bytes".to_string());
    }
    Ok(Some(script.to_string()))
}

fn custom_build_script_hash(script: &str) -> String {
    let digest = Sha256::digest(script.as_bytes());
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn generated_image_id(feature_ids: &[String], custom_script_hash: Option<&str>) -> String {
    let mut segments = feature_ids
        .iter()
        .filter(|feature| !feature.starts_with("script@"))
        .map(|feature| feature.replace('@', ""))
        .collect::<Vec<_>>();
    if let Some(hash) = custom_script_hash {
        segments.push(format!("script{hash}"));
    }
    if segments.is_empty() {
        return "base".to_string();
    }
    format!("dev-{}", segments.join("-"))
}

fn parse_runtime_selection(value: &str) -> Result<Option<RuntimeSelectionSpec>, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }

    let (runtime_raw, version_raw) = split_runtime_version(value.as_str());
    let runtime = find_runtime(runtime_raw)
        .ok_or_else(|| format!("unknown sandbox image runtime: {runtime_raw}"))?;
    let version = match version_raw {
        Some(version) => find_runtime_version(runtime, version).ok_or_else(|| {
            format!(
                "unknown {} version: {}; supported versions are {}",
                runtime.label,
                version,
                runtime
                    .versions
                    .iter()
                    .map(|item| item.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?,
        None => default_version(runtime),
    };
    Ok(Some(RuntimeSelectionSpec { runtime, version }))
}

fn parse_generated_image_id(image_id: &str) -> Option<ParsedImageId> {
    if image_id == "base" {
        return Some(ParsedImageId {
            selections: Vec::new(),
            custom_script_hash: None,
        });
    }
    let suffix = image_id.strip_prefix("dev-")?;
    if suffix.trim().is_empty() {
        return None;
    }

    let mut selections = Vec::new();
    let mut custom_script_hash = None;
    for segment in suffix.split('-') {
        if let Some(hash) = parse_custom_script_segment(segment) {
            if custom_script_hash.is_some() {
                return None;
            }
            custom_script_hash = Some(hash);
            continue;
        }
        let selection = parse_image_id_segment(segment)?;
        if selections
            .iter()
            .any(|existing: &RuntimeSelectionSpec| existing.runtime.id == selection.runtime.id)
        {
            return None;
        }
        selections.push(selection);
    }
    selections.sort_by_key(|selection| runtime_index(selection.runtime.id));
    Some(ParsedImageId {
        selections,
        custom_script_hash,
    })
}

fn parse_custom_script_segment(segment: &str) -> Option<String> {
    let hash = segment.trim().strip_prefix("script")?;
    if hash.len() < 8 || hash.len() > 32 || !hash.chars().all(|item| item.is_ascii_hexdigit()) {
        return None;
    }
    Some(hash.to_ascii_lowercase())
}

fn parse_image_id_segment(segment: &str) -> Option<RuntimeSelectionSpec> {
    let segment = segment.trim().to_ascii_lowercase();
    for runtime in IMAGE_RUNTIMES {
        let mut names = std::iter::once(runtime.id)
            .chain(runtime.aliases.iter().copied())
            .collect::<Vec<_>>();
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        for name in names {
            if let Some(version) = segment.strip_prefix(name) {
                let version = if version.is_empty() {
                    default_version(runtime)
                } else {
                    find_runtime_version(runtime, version)?
                };
                return Some(RuntimeSelectionSpec { runtime, version });
            }
        }
    }
    None
}

fn split_runtime_version(value: &str) -> (&str, Option<&str>) {
    if let Some((runtime, version)) = value.split_once('@').or_else(|| value.split_once(':')) {
        return (runtime.trim(), Some(version.trim()));
    }

    for runtime in IMAGE_RUNTIMES {
        for name in std::iter::once(runtime.id).chain(runtime.aliases.iter().copied()) {
            if let Some(version) = value.strip_prefix(name) {
                if !version.is_empty() {
                    return (runtime.id, Some(version.trim_matches(['-', '_', '@', ':'])));
                }
            }
        }
    }

    (value, None)
}

fn find_runtime(value: &str) -> Option<RuntimeSpec> {
    let value = value.trim();
    IMAGE_RUNTIMES
        .into_iter()
        .find(|runtime| runtime.id == value || runtime.aliases.iter().any(|alias| *alias == value))
}

fn find_runtime_version(runtime: RuntimeSpec, value: &str) -> Option<RuntimeVersionSpec> {
    let value = value.trim().trim_start_matches('v');
    runtime
        .versions
        .iter()
        .find(|version| {
            version.id == value
                || format!("{}{}", runtime.id, version.id) == value
                || format!("{}{}", runtime.label.to_ascii_lowercase(), version.id) == value
        })
        .copied()
}

fn default_version(runtime: RuntimeSpec) -> RuntimeVersionSpec {
    runtime
        .versions
        .iter()
        .find(|version| version.default)
        .copied()
        .unwrap_or(runtime.versions[0])
}

fn runtime_index(runtime_id: &str) -> usize {
    IMAGE_RUNTIMES
        .iter()
        .position(|runtime| runtime.id == runtime_id)
        .unwrap_or(usize::MAX)
}

fn selection_feature_token(selection: &RuntimeSelectionSpec) -> String {
    format!("{}@{}", selection.runtime.id, selection.version.id)
}

fn selection_label(selection: &RuntimeSelectionSpec) -> &'static str {
    selection.version.label
}

fn normalized_tag_prefix(config: &AppConfig) -> String {
    let prefix = config.image_tag_prefix.trim();
    if prefix.is_empty() {
        "chatos-sandbox-agent".to_string()
    } else {
        prefix.trim_end_matches(':').to_string()
    }
}

fn default_image_ref(config: &AppConfig, backend: SandboxBackendKind) -> String {
    match backend {
        SandboxBackendKind::Kata => config.kata_image.clone(),
        SandboxBackendKind::Docker | SandboxBackendKind::Mock => config.docker_image.clone(),
    }
}

fn container_cli(config: &AppConfig, backend: SandboxBackendKind) -> &str {
    match backend {
        SandboxBackendKind::Kata => config.kata_container_cli.as_str(),
        SandboxBackendKind::Docker | SandboxBackendKind::Mock => "docker",
    }
}

fn append_job_output(job: &mut SandboxImageJobRecord, text: &str) {
    job.output.push_str(text);
    if job.output.len() > MAX_JOB_OUTPUT_LEN {
        let keep_from = job.output.len().saturating_sub(MAX_JOB_OUTPUT_LEN);
        job.output = format!("...{}", &job.output[keep_from..]);
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
