mod authentication;
mod conversation_composer;
mod conversation_history;
mod host_lifecycle;
mod project_conversation;
mod workspace_index;

use authentication::{
    login, logout, register, restore_authentication_session, send_registration_code,
    AuthenticationManager,
};
use conversation_composer::send_conversation_draft;
use conversation_history::load_conversation_history;
use host_lifecycle::{
    host_diagnostics, host_lifecycle_snapshot, start_local_agent_host, stop_local_agent_host,
    HostManager,
};
use project_conversation::prepare_project_conversation;
use tauri::Manager;
use workspace_index::{list_local_projects, refresh_workspace_relations};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let authentication =
        AuthenticationManager::from_runtime().expect("failed to configure ChatOS authentication");
    let app = tauri::Builder::default()
        .manage(authentication)
        .manage(HostManager::default())
        .invoke_handler(tauri::generate_handler![
            restore_authentication_session,
            login,
            send_registration_code,
            register,
            logout,
            list_local_projects,
            refresh_workspace_relations,
            prepare_project_conversation,
            load_conversation_history,
            send_conversation_draft,
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
