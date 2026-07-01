fn main() {
    match chat_app_server_rs::maybe_run_process_isolation_exec_helper() {
        Ok(false) => {}
        Ok(true) => return,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(126);
        }
    }

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
