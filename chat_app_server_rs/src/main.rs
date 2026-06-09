#[tokio::main]
async fn main() {
    if let Err(err) = chat_app_server_rs::run_server_from_env().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
