use std::env;

#[tokio::main]
async fn main() {
    let origin = env::args().nth(1).unwrap_or_default();
    if let Err(error) = browser_cdp_bridge::run_native_host(origin).await {
        eprintln!("Browser Bridge native host stopped: {error}");
        std::process::exit(1);
    }
}
