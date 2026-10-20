mod host_lifecycle;

use host_lifecycle::{
    host_diagnostics, host_lifecycle_snapshot, start_local_agent_host, stop_local_agent_host,
    HostManager,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(HostManager::default())
        .invoke_handler(tauri::generate_handler![
            host_diagnostics,
            host_lifecycle_snapshot,
            start_local_agent_host,
            stop_local_agent_host,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build ChatOS Windows client");
    let app_handle = app.handle().clone();
    let exit_code = app.run_return(|_, _| {});
    tauri::async_runtime::block_on(async {
        app_handle.state::<HostManager>().shutdown().await;
    });
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
