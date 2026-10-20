mod host_lifecycle;

use host_lifecycle::{
    host_lifecycle_snapshot, start_local_agent_host, stop_local_agent_host, HostManager,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(HostManager::default())
        .invoke_handler(tauri::generate_handler![
            host_lifecycle_snapshot,
            start_local_agent_host,
            stop_local_agent_host,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ChatOS Windows client");
}
