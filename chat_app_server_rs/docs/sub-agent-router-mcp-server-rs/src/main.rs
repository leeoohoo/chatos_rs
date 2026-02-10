mod admin_server;
mod catalog;
mod config_store;
mod job_store;
mod marketplace;
mod mcp;
mod registry;
mod runner;
mod selector;
mod server;
mod types;
mod utils;

use crate::admin_server::{start_admin_server, AdminServerOptions};
use crate::catalog::SubAgentCatalog;
use crate::config_store::ConfigStore;
use crate::job_store::JobStore;
use crate::registry::AgentRegistry;
use crate::server::{register_tools, ServerOptions};
use crate::utils::{ensure_dir, generate_id, normalize_id, normalize_name, parse_args, resolve_state_dir};
use std::env;
use std::path::PathBuf;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

fn main() {
    let argv: Vec<String> = env::args().skip(1).collect();
    let args = parse_args(&argv);
    if args.flags.contains("help") || args.flags.contains("h") {
        print_help();
        return;
    }

    let one_day_ms = 24 * 60 * 60 * 1000;
    let server_name = normalize_name(
        args.values
            .get("name")
            .map(String::as_str)
            .unwrap_or("sub_agent_router"),
    );

    let session_id_arg = normalize_id(args.values.get("session-id").or_else(|| args.values.get("session")));
    let run_id_arg = normalize_id(args.values.get("run-id").or_else(|| args.values.get("run")));
    let session_id = if !session_id_arg.is_empty() {
        session_id_arg
    } else if let Ok(val) = env::var("MODEL_CLI_SESSION_ID") {
        val
    } else {
        generate_id("session")
    };
    let run_id = if !run_id_arg.is_empty() {
        run_id_arg
    } else {
        env::var("MODEL_CLI_RUN_ID").unwrap_or_default()
    };
    env::set_var("MODEL_CLI_SESSION_ID", &session_id);
    if !run_id.is_empty() {
        env::set_var("MODEL_CLI_RUN_ID", &run_id);
    }

    let state_dir = resolve_state_dir(&server_name);
    ensure_dir(&state_dir).expect("failed to create state dir");

    let db_path = args
        .values
        .get("db")
        .cloned()
        .or_else(|| env::var("MODEL_CLI_SUBAGENT_DB").ok())
        .unwrap_or_else(|| state_dir.join(format!("{server_name}.db.sqlite")).to_string_lossy().to_string());

    let registry_path = args
        .values
        .get("registry")
        .cloned()
        .or_else(|| env::var("MODEL_CLI_SUBAGENT_REGISTRY").ok())
        .unwrap_or_else(|| state_dir.join("subagents.json").to_string_lossy().to_string());

    let marketplace_path = args
        .values
        .get("marketplace")
        .cloned()
        .or_else(|| env::var("SUBAGENT_MARKETPLACE_PATH").ok())
        .or_else(|| env::var("MODEL_CLI_SUBAGENT_MARKETPLACE").ok())
        .unwrap_or_else(|| state_dir.join("marketplace.json").to_string_lossy().to_string());

    let admin_ui_root = args
        .values
        .get("admin-ui-root")
        .cloned()
        .or_else(|| env::var("SUBAGENT_ADMIN_UI_DIR").ok())
        .or_else(|| {
            let candidate = PathBuf::from("admin-ui").join("dist");
            if candidate.exists() {
                Some(candidate.to_string_lossy().to_string())
            } else {
                None
            }
        });

    let plugins_root = args
        .values
        .get("plugins-root")
        .cloned()
        .or_else(|| env::var("SUBAGENT_PLUGINS_ROOT").ok())
        .unwrap_or_else(|| state_dir.join("plugins").to_string_lossy().to_string());

    let timeout_ms = parse_number(args.values.get("timeout-ms")).or_else(|| parse_number_env("SUBAGENT_TIMEOUT_MS")).unwrap_or(one_day_ms);
    let max_output_bytes = parse_number(args.values.get("max-output-bytes")).or_else(|| parse_number_env("SUBAGENT_MAX_OUTPUT_BYTES")).unwrap_or(1024 * 1024);
    let llm_timeout_ms = parse_number(args.values.get("llm-timeout-ms")).or_else(|| parse_number_env("SUBAGENT_LLM_TIMEOUT_MS")).unwrap_or(one_day_ms);
    let llm_max_output_bytes = parse_number(args.values.get("llm-max-output-bytes")).or_else(|| parse_number_env("SUBAGENT_LLM_MAX_OUTPUT_BYTES")).unwrap_or(2 * 1024 * 1024);

    let registry_dir = PathBuf::from(&registry_path).parent().map(|p| p.to_path_buf()).unwrap_or(state_dir.clone());
    ensure_dir(&registry_dir).expect("failed to create registry dir");
    ensure_dir(&PathBuf::from(&plugins_root)).expect("failed to create plugins root");

    let registry_path_buf = PathBuf::from(&registry_path);
    let registry = AgentRegistry::new(&state_dir, Some(registry_path_buf.as_path()))
        .expect("failed to open registry");
    let catalog = SubAgentCatalog::new(
        registry,
        Some(PathBuf::from(&marketplace_path)),
        Some(PathBuf::from(&plugins_root)),
    );
    let catalog = Arc::new(Mutex::new(catalog));

    let config_store = ConfigStore::new(&db_path, Some(PathBuf::from(&marketplace_path)))
        .expect("failed to open config store");
    let config_store = Rc::new(RefCell::new(config_store));
    let _ = config_store.borrow().set_plugins_root(&plugins_root);
    let _ = config_store.borrow().set_marketplace_path(&marketplace_path);
    let _ = config_store.borrow().set_registry_path(&registry_path);
    let _ = config_store.borrow().set_db_path(&db_path);
    let _ = config_store.borrow().ensure_marketplace_file();

    let job_store = JobStore::new(&db_path, session_id.clone(), run_id.clone()).expect("failed to open job store");
    let job_store = Rc::new(RefCell::new(job_store));

    let async_registry = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    let mut server = crate::mcp::McpServer::new(server_name.clone(), "0.2.0");
    register_tools(
        &mut server,
        ServerOptions {
            server_name: server_name.clone(),
            catalog: catalog.clone(),
            job_store: job_store.clone(),
            config_store: config_store.clone(),
            default_session_id: session_id.clone(),
            default_run_id: run_id.clone(),
            db_path: db_path.clone(),
            timeout_ms,
            max_output_bytes,
            ai_timeout_ms: llm_timeout_ms,
            ai_max_output_bytes: llm_max_output_bytes,
            async_registry: async_registry.clone(),
        },
    );

    if let Some(port) = parse_number(args.values.get("admin-port")) {
        let host = args
            .values
            .get("admin-host")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let options = AdminServerOptions {
            host: host.clone(),
            port: port as u16,
            db_path: db_path.clone(),
            marketplace_path: marketplace_path.clone(),
            plugins_root: plugins_root.clone(),
            registry_path: registry_path.clone(),
            default_session_id: session_id.clone(),
            default_run_id: run_id.clone(),
            catalog: catalog.clone(),
            admin_ui_root: admin_ui_root.clone().map(PathBuf::from),
        };
        std::thread::spawn(move || start_admin_server(options));
    }

    if let Err(err) = server.run_stdio() {
        eprintln!("[{server_name}] Sub-agent router crashed: {err}");
        std::process::exit(1);
    }
}

fn parse_number(value: Option<&String>) -> Option<i64> {
    value.and_then(|v| v.parse::<i64>().ok())
}

fn parse_number_env(key: &str) -> Option<i64> {
    env::var(key).ok().and_then(|v| v.parse::<i64>().ok())
}

fn print_help() {
    println!(
        "Usage: sub-agent-router-mcp-server-rs [options]\n\nOptions:\n  --name <id>             MCP server name (default sub_agent_router)\n  --db <path>             Database file path\n  --registry <path>       Registry JSON path\n  --marketplace <path>    Marketplace JSON path\n  --plugins-root <path>   Root to resolve marketplace plugin sources\n  --session-id <id>       Session ID override\n  --run-id <id>           Run ID override\n  --timeout-ms <ms>       Command timeout in milliseconds (default 86400000)\n  --max-output-bytes <n>  Max stdout/stderr capture per stream (default 1048576)\n  --llm-timeout-ms <ms>   AI command timeout (default 86400000)\n  --llm-max-output-bytes  AI max stdout/stderr bytes (default 2097152)\n  --admin-port <n>        Admin UI port (disabled by default)\n  --admin-host <host>     Admin UI host (default 127.0.0.1)\n  --admin-ui-root <path>  Admin UI static root (default ./admin-ui/dist if present)\n  --help                  Show help"
    );
}
