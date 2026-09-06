use std::{
    env,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use browser_cdp_bridge::{
    BridgeReady, BridgeServer, BridgeServerConfig, install_active_bridge_locator,
    install_native_host, platform_data_dir, remove_active_bridge_locator_if_owned, run_native_host,
    uninstall_native_host,
};
use browser_cdp_core::{BrowserBackendFactory, BrowserRuntime};
use browser_cdp_direct::DirectBackendFactory;
use browser_cdp_extension::ExtensionBackendFactory;
use fs2::FileExt;
use tokio::{sync::oneshot, task::JoinHandle};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let command = env::args().nth(1).unwrap_or_else(|| "mcp".into());
    if command.starts_with("chrome-extension://") {
        let exit_code = match run_native_host(command).await {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Browser MCP native host stopped: {error}");
                1
            }
        };
        std::process::exit(exit_code);
    }
    let exit_code = match command.as_str() {
        "mcp" => run_mcp().await,
        "doctor" => {
            doctor();
            0
        }
        "version" | "--version" | "-V" => {
            println!("chatos-browser-cdp {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "install-native-host" => install_native_host_command().await,
        "uninstall-native-host" => uninstall_native_host_command().await,
        "help" | "--help" | "-h" => {
            println!(
                "Usage: chatos-browser-cdp [mcp|doctor|version|install-native-host|uninstall-native-host]"
            );
            0
        }
        other => {
            eprintln!("unknown command: {other}");
            2
        }
    };
    std::process::exit(exit_code);
}

async fn run_mcp() -> i32 {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let data_dir = env_path("CHATOS_PLUGIN_DATA_DIR", "chatos-browser-cdp/data");
    let artifact_dir = env_path("CHATOS_PLUGIN_ARTIFACT_DIR", "chatos-browser-cdp/artifacts");
    let bridge_data_dir = data_dir.join("browser-bridge");
    let mut factories: Vec<Arc<dyn BrowserBackendFactory>> =
        vec![Arc::new(DirectBackendFactory::new(data_dir))];
    let mut managed_bridge = None;
    if env::var_os("CHATOS_BROWSER_BRIDGE_ENDPOINT").is_some() {
        tracing::warn!(
            "using the development-only external Browser Bridge override; production uses the Browser MCP owned bridge"
        );
        factories.push(Arc::new(ExtensionBackendFactory::from_environment()));
    } else if let Some(extension_id) = configured_extension_id() {
        match ManagedBridgeRuntime::start(extension_id, bridge_data_dir).await {
            Ok(bridge) => {
                factories.push(Arc::new(bridge.extension_factory()));
                managed_bridge = Some(bridge);
            }
            Err(error) => {
                tracing::warn!(%error, "self-managed Existing Chrome bridge is unavailable")
            }
        }
    }
    let runtime = Arc::new(BrowserRuntime::new(factories, artifact_dir));
    let result = browser_cdp_mcp::serve_stdio(runtime).await;
    if let Some(bridge) = managed_bridge {
        bridge.shutdown().await;
    }
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("MCP stdio failure: {error}");
            1
        }
    }
}

fn doctor() {
    let candidates = chrome_candidates();
    let found = candidates.iter().find(|path| path.is_file());
    println!(
        "{}",
        serde_json::json!({
            "ok": found.is_some(),
            "version": env!("CARGO_PKG_VERSION"),
            "managed_chrome": found.map(|path| path.display().to_string()),
            "extension_id_configured": configured_extension_id(),
            "external_bridge_override_present": env::var_os("CHATOS_BROWSER_BRIDGE_ENDPOINT").is_some(),
            "external_bridge_credential_present": env::var_os("CHATOS_BROWSER_BRIDGE_TOKEN").is_some()
                || env::var_os("CHATOS_BROWSER_BRIDGE_CREDENTIAL_FILE").is_some(),
            "data_dir": env_path("CHATOS_PLUGIN_DATA_DIR", "chatos-browser-cdp/data"),
            "artifact_dir": env_path("CHATOS_PLUGIN_ARTIFACT_DIR", "chatos-browser-cdp/artifacts")
        })
    );
}

struct ManagedBridgeRuntime {
    ready: BridgeReady,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), String>>,
    _lease: File,
    locator_file: PathBuf,
}

impl ManagedBridgeRuntime {
    async fn start(extension_id: String, bridge_dir: PathBuf) -> Result<Self, String> {
        let locator_dir = platform_data_dir();
        tokio::fs::create_dir_all(&locator_dir)
            .await
            .map_err(|error| {
                format!("could not create Browser Bridge locator directory: {error}")
            })?;
        tokio::fs::create_dir_all(&bridge_dir)
            .await
            .map_err(|error| format!("could not create Browser Bridge directory: {error}"))?;
        let lease_path = locator_dir.join("bridge.lock");
        let lease = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lease_path)
            .map_err(|error| format!("could not open Browser Bridge lease: {error}"))?;
        lease.try_lock_exclusive().map_err(|_| {
            "another Browser MCP process already owns the Existing Chrome bridge".to_owned()
        })?;
        let executable = env::current_exe()
            .map_err(|error| format!("could not resolve Browser MCP executable: {error}"))?;
        install_native_host(&executable, &extension_id).await?;
        let (server, ready) =
            BridgeServer::bind(BridgeServerConfig::development(bridge_dir, extension_id)).await?;
        let locator_file = install_active_bridge_locator(&locator_dir, &ready.state_file).await?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(server.serve_until(async {
            let _ = shutdown_rx.await;
        }));
        Ok(Self {
            ready,
            shutdown: Some(shutdown_tx),
            task,
            _lease: lease,
            locator_file,
        })
    }

    fn extension_factory(&self) -> ExtensionBackendFactory {
        ExtensionBackendFactory::from_credential_file(
            self.ready.browser_endpoint.clone(),
            self.ready.mcp_credential_file.clone(),
        )
    }

    async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let mut task = self.task;
        if tokio::time::timeout(Duration::from_secs(2), &mut task)
            .await
            .is_err()
        {
            task.abort();
        }
        remove_active_bridge_locator_if_owned(&self.locator_file, &self.ready.state_file).await;
        remove_if_owned(&self.ready.state_file).await;
        remove_if_owned(&self.ready.mcp_credential_file).await;
    }
}

fn configured_extension_id() -> Option<String> {
    env::var("CHATOS_BROWSER_EXTENSION_ID")
        .ok()
        .or_else(|| option_env!("CHATOS_BROWSER_EXTENSION_ID").map(str::to_owned))
        .filter(|value| is_extension_id(value))
}

async fn install_native_host_command() -> i32 {
    let Some(extension_id) = env::args().nth(2).or_else(configured_extension_id) else {
        eprintln!("install-native-host requires a Chrome extension ID");
        return 2;
    };
    let executable = match env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            eprintln!("could not resolve Browser MCP executable: {error}");
            return 1;
        }
    };
    match install_native_host(&executable, &extension_id).await {
        Ok(installed) => {
            println!(
                "{}",
                serde_json::json!({"installed": installed.manifest_paths})
            );
            0
        }
        Err(error) => {
            eprintln!("native host installation failed: {error}");
            1
        }
    }
}

async fn uninstall_native_host_command() -> i32 {
    let Some(extension_id) = env::args().nth(2).or_else(configured_extension_id) else {
        eprintln!("uninstall-native-host requires a Chrome extension ID");
        return 2;
    };
    match uninstall_native_host(&extension_id).await {
        Ok(removed) => {
            println!("{}", serde_json::json!({"removed": removed}));
            0
        }
        Err(error) => {
            eprintln!("native host uninstall failed: {error}");
            1
        }
    }
}

async fn remove_if_owned(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

fn is_extension_id(value: &str) -> bool {
    value.len() == 32
        && value
            .chars()
            .all(|character| matches!(character, 'a'..='p'))
}

fn env_path(name: &str, fallback_suffix: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join(fallback_suffix))
}

fn chrome_candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
            "/Applications/Chromium.app/Contents/MacOS/Chromium".into(),
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge".into(),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            "/usr/bin/google-chrome".into(),
            "/usr/bin/chromium".into(),
            "/usr/bin/chromium-browser".into(),
            "/usr/bin/microsoft-edge".into(),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        for root in [
            env::var_os("PROGRAMFILES"),
            env::var_os("PROGRAMFILES(X86)"),
            env::var_os("LOCALAPPDATA"),
        ]
        .into_iter()
        .flatten()
        {
            let root = PathBuf::from(root);
            candidates.push(root.join("Google/Chrome/Application/chrome.exe"));
            candidates.push(root.join("Microsoft/Edge/Application/msedge.exe"));
        }
        candidates
    }
}
