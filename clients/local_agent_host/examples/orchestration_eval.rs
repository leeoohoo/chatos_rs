#[path = "orchestration_eval/checks.rs"]
mod checks;
#[path = "orchestration_eval/runtime.rs"]
mod runtime;
#[path = "orchestration_eval/smoke.rs"]
mod smoke;

use chatos_ai_runtime::ModelRuntimeConfig;
use chatos_local_agent_protocol::{LocalCapabilityPolicySnapshot, LOCAL_AGENT_PROTOCOL_VERSION};
use serde_json::json;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

struct Options {
    live: bool,
    list: bool,
    all: bool,
    loopback_http: bool,
    case: Option<String>,
    fixture: Option<PathBuf>,
    dataset: PathBuf,
    output: Option<PathBuf>,
}
fn options() -> Result<Options, String> {
    let mut result = Options {
        live: false,
        list: false,
        all: false,
        loopback_http: false,
        case: None,
        fixture: None,
        output: None,
        dataset: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../evals/task-orchestration/scenarios.json"),
    };
    let mut args = env::args().skip(1);
    let mut mode = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--live" | "--smoke" | "--list" => {
                if mode.replace(arg.clone()).is_some() {
                    return Err("choose_one_eval_mode".into());
                }
                result.live = arg == "--live";
                result.list = arg == "--list";
            }
            "--all" => result.all = true,
            "--allow-loopback-http" => result.loopback_http = true,
            "--case" => result.case = Some(args.next().ok_or("missing_case_id")?),
            "--fixture" => result.fixture = Some(args.next().ok_or("missing_fixture_path")?.into()),
            "--dataset" => result.dataset = args.next().ok_or("missing_dataset_path")?.into(),
            "--output" => result.output = Some(args.next().ok_or("missing_output_path")?.into()),
            "--help" => {
                println!("orchestration_eval [--smoke | --list | --live (--case ID | --all)] [--fixture FILE] [--dataset FILE] [--output NEW_FILE] [--allow-loopback-http]\nLive mode uses only explicit CHATOS_EVAL_BASE_URL/API_KEY/MODEL/PROVIDER/THINKING_LEVEL environment variables.\nNo business Task is executed. Smoke replay is NOT model-quality evidence.");
                std::process::exit(0);
            }
            _ => return Err("unknown_eval_argument".into()),
        }
    }
    if result.live && result.all == result.case.is_some()
        || !result.live && (result.all || result.case.is_some() || result.loopback_http)
    {
        return Err("live_requires_exactly_case_or_all".into());
    }
    Ok(result)
}
fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T, String> {
    if fs::metadata(path)
        .map_err(|_| "eval_input_unavailable")?
        .len()
        > 4 * 1024 * 1024
    {
        return Err("eval_input_too_large".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| "eval_input_unavailable")?)
        .map_err(|_| "invalid_eval_json".into())
}
fn validate_endpoint(endpoint: &str, allow_loopback: bool) -> Result<(), String> {
    let url = reqwest::Url::parse(endpoint).map_err(|_| "invalid_eval_endpoint")?;
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host == "[::1]"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
        || !(url.scheme() == "https" || allow_loopback && local && url.scheme() == "http")
    {
        return Err("eval_requires_https_or_explicit_loopback_http".into());
    }
    Ok(())
}
fn live_config(allow_loopback: bool) -> Result<ModelRuntimeConfig, String> {
    let endpoint = env::var("CHATOS_EVAL_BASE_URL").map_err(|_| "missing_eval_base_url")?;
    validate_endpoint(&endpoint, allow_loopback)?;
    let key = env::var("CHATOS_EVAL_API_KEY").map_err(|_| "missing_eval_api_key")?;
    let model = env::var("CHATOS_EVAL_MODEL").map_err(|_| "missing_eval_model")?;
    if key.trim().is_empty()
        || key.chars().any(char::is_control)
        || model.trim().is_empty()
        || key.len() > 4096
        || model.len() > 256
    {
        return Err("invalid_eval_model_or_key".into());
    }
    Ok(ModelRuntimeConfig::openai_compatible(
        endpoint,
        key,
        model,
        env::var("CHATOS_EVAL_PROVIDER").unwrap_or_else(|_| "gpt".into()),
    )
    .with_responses_support(true)
    .with_max_output_tokens(Some(4096))
    .with_max_transient_retries(Some(0))
    .with_request_body_limit_bytes(Some(2 * 1024 * 1024))
    .with_thinking_level(env::var("CHATOS_EVAL_THINKING_LEVEL").ok()))
}

async fn run() -> Result<bool, String> {
    let options = options()?;
    let data: checks::Dataset = read_json(&options.dataset)?;
    data.validate()?;
    if options.list {
        for case in &data.scenarios {
            println!("{}\t{}\t{}", case.id, case.category, case.domain);
        }
        return Ok(true);
    }
    // Reject an existing destination before invoking even one model request.
    let mut output_file = options
        .output
        .as_ref()
        .map(|path| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|_| "eval_output_requires_new_writable_file")
        })
        .transpose()?;
    let snapshots: Vec<LocalCapabilityPolicySnapshot> =
        read_json(&options.fixture.ok_or("missing_frozen_fixture")?)?;
    let config = if options.live {
        live_config(options.loopback_http)?
    } else {
        ModelRuntimeConfig::openai_compatible(
            "https://example.invalid",
            "unused-offline",
            "eval-placeholder",
            "gpt",
        )
    };
    let (reports, negatives) = if options.live {
        let cases = data
            .scenarios
            .iter()
            .filter(|case| options.all || options.case.as_deref() == Some(&case.id))
            .collect::<Vec<_>>();
        if cases.is_empty() {
            return Err("unknown_eval_case".into());
        }
        let mut reports = Vec::new();
        let mut total_requests = 0;
        for case in cases {
            if total_requests >= 800 {
                return Err("global_eval_request_budget_exceeded".into());
            }
            eprintln!(
                "Evaluating {} (planning only; semantic review remains required)",
                case.id
            );
            let mut session = runtime::Session::new(
                &snapshots,
                config.clone(),
                if case.seed.is_some() {
                    "请执行下面的原始工作"
                } else {
                    &case.prompt
                },
            )
            .await?;
            if let Some(seed) = &case.seed {
                session.seed_followup(seed, &case.prompt).await?;
            }
            let profile = runtime::live_profile(session.control.clone())?;
            smoke::drive(&mut session, profile.as_ref(), 16).await;
            total_requests += session.requests;
            let transport_failure = session.failure.is_some();
            reports.push(
                checks::report(case, &data.capability_profiles[&case.capabilities], session)
                    .await?,
            );
            // No blind retry or continuation to the next case after an uncertain failure.
            if transport_failure {
                break;
            }
        }
        (reports, Vec::new())
    } else {
        smoke::run(&data, &snapshots, &config).await?
    };
    let structural_pass = reports.iter().all(|report| report.structural_pass)
        && negatives.iter().all(|control| control.passed);
    let value = json!({
        "schema_version": 1, "protocol_version": LOCAL_AGENT_PROTOCOL_VERSION,
        "mode": if options.live { "live_planning" } else { "offline_replay" },
        "model_quality_proven": false, "semantic_review_required": true,
        "structural_pass": structural_pass,
        "requested_all": options.all,
        "dataset_scenarios": data.scenarios.len(), "completed_reports": reports.len(),
        "frozen_revision": snapshots.iter().find(|s| s.profile_key == "main_chat").map(|s| &s.capability_policy_revision),
        "limits": {"requests_per_case": 16, "requests_total": 800, "seconds_per_request": 45,
            "seconds_per_case": 180, "max_output_tokens": 4096, "automatic_retries": 0},
        "isolation": {"sqlite": "memory", "business_executors_registered": false, "business_runs_executed": false,
            "app_credentials_loaded": false, "seed_phase_excluded": true},
        "cases": reports, "negative_controls": negatives,
    });
    let mut rendered =
        serde_json::to_string_pretty(&value).map_err(|_| "report_serialization_failed")?;
    if options.live {
        let escaped =
            serde_json::to_string(&config.api_key).map_err(|_| "report_redaction_failed")?;
        rendered = rendered.replace(&escaped[1..escaped.len() - 1], "[REDACTED]");
    }
    if let Some(file) = &mut output_file {
        file.write_all(rendered.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|_| "report_write_failed")?;
    } else {
        println!("{rendered}");
    }
    Ok(structural_pass)
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(code) => {
            eprintln!("Evaluation stopped: {code}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_constraints_are_explicit() {
        assert!(validate_endpoint("https://example.com/v1", false).is_ok());
        for url in [
            "http://example.com/v1",
            "https://user:password@example.com/v1",
            "https://example.com/v1?token=secret",
            "https://example.com/v1#fragment",
            "file:///tmp/test",
        ] {
            assert!(validate_endpoint(url, true).is_err());
        }
        for url in [
            "http://localhost:9000/v1",
            "http://127.0.0.1:9000/v1",
            "http://[::1]:9000/v1",
        ] {
            assert!(validate_endpoint(url, false).is_err());
            assert!(validate_endpoint(url, true).is_ok());
        }
    }
}
