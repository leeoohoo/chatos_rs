use std::{env, path::PathBuf};

use browser_cdp_bridge::{BridgeServer, BridgeServerConfig, default_data_dir};

#[tokio::main]
async fn main() {
    let mut extension_id = env::var("CHATOS_BROWSER_EXTENSION_ID").ok();
    let mut data_dir = default_data_dir();
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--extension-id" => extension_id = args.next(),
            "--data-dir" => {
                data_dir = args
                    .next()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| usage_and_exit())
            }
            "--help" | "-h" => usage_and_exit(),
            _ => usage_and_exit(),
        }
    }
    let extension_id = extension_id.unwrap_or_else(|| usage_and_exit());
    let (server, ready) =
        BridgeServer::bind(BridgeServerConfig::development(data_dir, extension_id))
            .await
            .unwrap_or_else(|error| {
                eprintln!("Browser Bridge startup failed: {error}");
                std::process::exit(1);
            });
    println!(
        "{}",
        serde_json::to_string(&ready).expect("ready state serializes")
    );
    server.serve().await.unwrap_or_else(|error| {
        eprintln!("Browser Bridge stopped: {error}");
        std::process::exit(1);
    });
}

fn usage_and_exit() -> ! {
    eprintln!(
        "Usage: chatos-browser-bridge --extension-id <32-character Chrome ID> [--data-dir <path>]"
    );
    std::process::exit(2)
}
