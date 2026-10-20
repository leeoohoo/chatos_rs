mod attachment_tool_worker;
mod authentication;
mod control_plane;
mod conversation_attachments;
mod conversation_composer;
mod conversation_history;
mod conversation_realtime;
mod conversation_runtime_settings;
mod host_lifecycle;
mod project_conversation;
mod run_inspector;
mod workspace_index;

use attachment_tool_worker::AttachmentToolWorker;
use authentication::{
    login, logout, register, restore_authentication_session, send_registration_code,
    AuthenticationManager,
};
use conversation_attachments::{
    activate_conversation_attachment_scope, attachment_protocol,
    deactivate_conversation_attachment_scope, discard_conversation_attachments,
    import_conversation_clipboard, load_persisted_conversation_attachments,
    pick_conversation_attachments, preview_conversation_attachment_text,
    preview_persisted_conversation_attachment_text, AttachmentDragState, AttachmentManager,
};
use conversation_composer::send_conversation_draft;
use conversation_history::load_conversation_history;
use conversation_realtime::poll_conversation_realtime;
use conversation_runtime_settings::{
    load_conversation_runtime_settings, select_conversation_model,
    select_conversation_reasoning_level, select_conversation_remote_connection,
    ConversationRuntimeSettingsManager,
};
use host_lifecycle::{
    host_diagnostics, host_lifecycle_snapshot, start_local_agent_host, stop_local_agent_host,
    HostManager,
};
use project_conversation::prepare_project_conversation;
use run_inspector::{
    cancel_task_inspector, load_message_task_graph, load_run_inspector, load_task_inspector,
    restart_task_inspector, retry_task_inspector,
};
use tauri::{DragDropEvent, Emitter, Manager, WebviewEvent};
use workspace_index::{list_local_projects, refresh_workspace_relations};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let authentication =
        AuthenticationManager::from_runtime().expect("failed to configure ChatOS authentication");
    let attachment_manager =
        AttachmentManager::from_runtime().expect("failed to configure attachment vault");
    let protocol_attachment_manager = attachment_manager.clone();
    let drop_attachment_manager = attachment_manager.clone();
    let app = tauri::Builder::default()
        .manage(authentication)
        .manage(attachment_manager)
        .manage(AttachmentToolWorker::default())
        .manage(HostManager::default())
        .manage(ConversationRuntimeSettingsManager::default())
        .register_uri_scheme_protocol("chatos-attachment", move |_context, request| {
            attachment_protocol(protocol_attachment_manager.clone(), request)
        })
        .on_webview_event(move |webview, event| {
            let emit_drag_state = |targeted| {
                let _ = webview.emit(
                    "conversation-attachment-drag-state",
                    AttachmentDragState {
                        scope: drop_attachment_manager.active_scope(),
                        targeted,
                    },
                );
            };
            match event {
                WebviewEvent::DragDrop(DragDropEvent::Enter { .. }) => emit_drag_state(true),
                WebviewEvent::DragDrop(DragDropEvent::Over { .. }) => emit_drag_state(true),
                WebviewEvent::DragDrop(DragDropEvent::Leave) => emit_drag_state(false),
                WebviewEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                    emit_drag_state(false);
                    let Some(scope) = drop_attachment_manager.active_scope() else {
                        return;
                    };
                    let manager = drop_attachment_manager.clone();
                    let webview = webview.clone();
                    let paths = paths.clone();
                    tauri::async_runtime::spawn(async move {
                        let result = manager.stage_paths(scope.clone(), paths).await;
                        if manager.active_scope().as_ref() != Some(&scope) {
                            manager.discard_import_result(&result);
                            return;
                        }
                        let _ = webview.emit("conversation-attachments-staged", result);
                    });
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            restore_authentication_session,
            login,
            send_registration_code,
            register,
            logout,
            list_local_projects,
            refresh_workspace_relations,
            prepare_project_conversation,
            load_message_task_graph,
            load_task_inspector,
            load_run_inspector,
            retry_task_inspector,
            cancel_task_inspector,
            restart_task_inspector,
            load_conversation_history,
            poll_conversation_realtime,
            send_conversation_draft,
            activate_conversation_attachment_scope,
            deactivate_conversation_attachment_scope,
            pick_conversation_attachments,
            import_conversation_clipboard,
            discard_conversation_attachments,
            preview_conversation_attachment_text,
            load_persisted_conversation_attachments,
            preview_persisted_conversation_attachment_text,
            load_conversation_runtime_settings,
            select_conversation_model,
            select_conversation_reasoning_level,
            select_conversation_remote_connection,
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
        app_handle.state::<AttachmentToolWorker>().reset().await;
        app_handle.state::<HostManager>().shutdown().await;
    });
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
