// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

fn main() {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("Failed to start tokio runtime: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = runtime.block_on(chat_app_server_rs::run_server_from_env()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
