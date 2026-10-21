mod access_control;
mod ask_user;
mod attachment_tool_worker;
mod authentication;
mod connector_state;
mod control_plane;
mod conversation_attachments;
mod conversation_composer;
mod conversation_history;
mod conversation_realtime;
mod conversation_runtime_settings;
mod host_lifecycle;
mod local_terminal;
mod media_studio;
mod platform_execution_context;
mod plugin_application;
mod project_conversation;
mod project_creation;
mod project_favorite;
mod project_filesystem;
mod project_git;
mod project_removal;
mod project_run;
mod remote_connection;
mod run_inspector;
mod story_studio;
mod system_permissions;
mod terminal_runtime;
mod terminal_tool_worker;
mod tool_approval;
mod workspace_index;

use access_control::load_access_control;
use ask_user::{cancel_ask_user_prompt, load_ask_user_prompts, submit_ask_user_prompt};
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
use local_terminal::{
    close_local_terminal, discard_local_terminal_presentation, interrupt_local_terminal,
    poll_local_terminal, resize_local_terminal, send_local_terminal_input, start_local_terminal,
    LocalTerminalRegistry,
};
use media_studio::{
    cancel_media_studio_video_generation, discard_media_studio_presentation,
    discard_media_studio_reference_images, discard_media_studio_video_reference_audio,
    generate_media_studio_image, list_media_studio_models, load_media_studio_image_history,
    load_media_studio_video_history, media_studio_protocol, pick_media_studio_reference_images,
    pick_media_studio_video_reference_audio, poll_media_studio_video_generation,
    stage_media_studio_history_references, start_media_studio_video_generation, MediaStudioManager,
};
use plugin_application::{
    discard_plugin_application_presentation, handle_plugin_application_bridge_request,
    launch_plugin_application, list_plugin_applications, PluginApplicationManager,
};
use project_conversation::prepare_project_conversation;
use project_creation::{
    create_local_project, discard_project_directory_selection, list_project_types,
    pick_project_directory, ProjectCreationManager,
};
use project_favorite::{load_project_favorites, set_project_favorite, ProjectFavoriteStore};
use project_filesystem::{
    create_project_entry, delete_project_entry, load_project_directory, read_project_file,
    search_project_content, search_project_entries, write_project_file,
};
use project_git::{
    commit_project_git, create_project_git_branch, initialize_project_git_repository,
    load_project_git_diff, load_project_git_snapshot, merge_project_git_branch, pull_project_git,
    push_project_git, remove_project_git_remote, save_project_git_remote, stage_project_git_paths,
    switch_project_git_branch, unstage_project_git_paths,
};
use project_removal::remove_local_project;
use project_run::{
    analyze_project_run, delete_project_run_instance, load_project_run_state,
    poll_project_run_changes, set_project_run_default_target, start_project_run, stop_project_run,
    update_project_run_environment, ProjectRunManager, ProjectRunPreferencesStore,
};
pub use remote_connection::run_ssh_askpass_helper_if_requested;
use remote_connection::{
    close_remote_terminal, create_remote_connection, create_remote_directory,
    delete_remote_connection, delete_remote_entry, discard_remote_credential_file_selections,
    discard_remote_file_selections, discard_remote_terminal_presentation, download_remote_file,
    get_remote_connection, interrupt_remote_terminal, list_remote_connections,
    list_remote_directory, pick_remote_credential_file, pick_remote_download_destination,
    pick_remote_upload_file, poll_remote_terminal, rename_remote_entry, resize_remote_terminal,
    send_remote_terminal_input, start_remote_terminal, test_remote_connection_draft,
    test_saved_remote_connection, update_remote_connection, upload_remote_file,
    RemoteConnectionManager,
};
use run_inspector::{
    cancel_task_inspector, load_message_task_graph, load_run_inspector, load_task_inspector,
    restart_task_inspector, retry_task_inspector,
};
use story_studio::{
    abandon_story_planning, archive_story_completed_video, clear_story_last_frame,
    confirm_latest_story_frames, confirm_latest_story_resource_images, confirm_story_image,
    create_story_project, discard_story_studio_presentation, generate_story_asset_image,
    generate_story_frame_image, import_local_story_image, import_story_history_image,
    load_story_planning_history, load_story_project, load_story_projects,
    poll_story_video_generation, resume_story_planning, start_story_planning,
    start_story_video_generation, story_studio_protocol, update_story_project_settings,
    update_story_project_source, update_story_resource_prompt, update_story_segment,
    update_story_segment_structure, update_story_video_guidance, StoryStudioManager,
};
use system_permissions::{load_system_permissions, request_system_permission};
use tauri::{DragDropEvent, Emitter, Manager, WebviewEvent};
use terminal_tool_worker::TerminalToolWorker;
use tool_approval::journal::ApprovalDecisionJournal;
use tool_approval::settings::ApprovalSettingsStore;
use tool_approval::{
    decide_tool_approval, list_tool_approvals, load_tool_approval_settings,
    update_tool_approval_mode, update_tool_approval_model,
};
use workspace_index::{list_local_projects, refresh_workspace_relations};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let authentication =
        AuthenticationManager::from_runtime().expect("failed to configure ChatOS authentication");
    let attachment_manager =
        AttachmentManager::from_runtime().expect("failed to configure attachment vault");
    let media_studio_manager =
        MediaStudioManager::from_runtime().expect("failed to configure media studio");
    let story_studio_manager =
        StoryStudioManager::from_runtime().expect("failed to configure story studio");
    let protocol_attachment_manager = attachment_manager.clone();
    let protocol_media_studio_manager = media_studio_manager.clone();
    let protocol_story_studio_manager = story_studio_manager.clone();
    let drop_attachment_manager = attachment_manager.clone();
    let app = tauri::Builder::default()
        .manage(authentication)
        .manage(attachment_manager)
        .manage(media_studio_manager)
        .manage(story_studio_manager)
        .manage(AttachmentToolWorker::default())
        .manage(TerminalToolWorker::default())
        .manage(HostManager::default())
        .manage(LocalTerminalRegistry::default())
        .manage(ConversationRuntimeSettingsManager::default())
        .manage(ProjectCreationManager::default())
        .manage(ProjectRunManager::default())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let journal = ApprovalDecisionJournal::open(
                data_dir.join("local-agent").join("approval-decisions"),
            )
            .map_err(std::io::Error::other)?;
            app.manage(journal);
            app.manage(ApprovalSettingsStore::new(data_dir.clone()));
            app.manage(ProjectRunPreferencesStore::new(data_dir.clone()));
            app.manage(RemoteConnectionManager::new(data_dir.clone()));
            app.manage(ProjectFavoriteStore::new(data_dir.clone()));
            app.manage(PluginApplicationManager::new(data_dir));
            Ok(())
        })
        .register_uri_scheme_protocol("chatos-attachment", move |_context, request| {
            attachment_protocol(protocol_attachment_manager.clone(), request)
        })
        .register_uri_scheme_protocol("chatos-media", move |_context, request| {
            media_studio_protocol(protocol_media_studio_manager.clone(), request)
        })
        .register_uri_scheme_protocol("chatos-story", move |_context, request| {
            story_studio_protocol(protocol_story_studio_manager.clone(), request)
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
            list_project_types,
            pick_project_directory,
            discard_project_directory_selection,
            create_local_project,
            refresh_workspace_relations,
            prepare_project_conversation,
            load_project_directory,
            read_project_file,
            search_project_entries,
            search_project_content,
            write_project_file,
            create_project_entry,
            delete_project_entry,
            load_project_git_snapshot,
            load_project_git_diff,
            initialize_project_git_repository,
            stage_project_git_paths,
            unstage_project_git_paths,
            commit_project_git,
            switch_project_git_branch,
            create_project_git_branch,
            merge_project_git_branch,
            save_project_git_remote,
            remove_project_git_remote,
            pull_project_git,
            push_project_git,
            remove_local_project,
            load_project_favorites,
            set_project_favorite,
            analyze_project_run,
            load_project_run_state,
            poll_project_run_changes,
            start_project_run,
            stop_project_run,
            delete_project_run_instance,
            set_project_run_default_target,
            update_project_run_environment,
            load_message_task_graph,
            load_task_inspector,
            load_run_inspector,
            retry_task_inspector,
            cancel_task_inspector,
            restart_task_inspector,
            load_conversation_history,
            load_ask_user_prompts,
            submit_ask_user_prompt,
            cancel_ask_user_prompt,
            list_tool_approvals,
            decide_tool_approval,
            load_tool_approval_settings,
            update_tool_approval_mode,
            update_tool_approval_model,
            load_system_permissions,
            request_system_permission,
            load_access_control,
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
            list_remote_connections,
            get_remote_connection,
            create_remote_connection,
            update_remote_connection,
            delete_remote_connection,
            test_remote_connection_draft,
            test_saved_remote_connection,
            pick_remote_credential_file,
            discard_remote_credential_file_selections,
            list_remote_directory,
            create_remote_directory,
            rename_remote_entry,
            delete_remote_entry,
            pick_remote_upload_file,
            pick_remote_download_destination,
            discard_remote_file_selections,
            upload_remote_file,
            download_remote_file,
            start_remote_terminal,
            send_remote_terminal_input,
            resize_remote_terminal,
            interrupt_remote_terminal,
            poll_remote_terminal,
            close_remote_terminal,
            discard_remote_terminal_presentation,
            start_local_terminal,
            send_local_terminal_input,
            resize_local_terminal,
            interrupt_local_terminal,
            poll_local_terminal,
            close_local_terminal,
            discard_local_terminal_presentation,
            list_plugin_applications,
            list_media_studio_models,
            load_media_studio_image_history,
            load_media_studio_video_history,
            pick_media_studio_reference_images,
            pick_media_studio_video_reference_audio,
            stage_media_studio_history_references,
            discard_media_studio_reference_images,
            discard_media_studio_video_reference_audio,
            discard_media_studio_presentation,
            generate_media_studio_image,
            start_media_studio_video_generation,
            poll_media_studio_video_generation,
            cancel_media_studio_video_generation,
            load_story_projects,
            load_story_project,
            load_story_planning_history,
            create_story_project,
            update_story_project_settings,
            update_story_project_source,
            update_story_resource_prompt,
            update_story_segment,
            update_story_segment_structure,
            confirm_story_image,
            clear_story_last_frame,
            confirm_latest_story_resource_images,
            confirm_latest_story_frames,
            import_local_story_image,
            import_story_history_image,
            generate_story_asset_image,
            generate_story_frame_image,
            start_story_video_generation,
            poll_story_video_generation,
            update_story_video_guidance,
            archive_story_completed_video,
            discard_story_studio_presentation,
            start_story_planning,
            resume_story_planning,
            abandon_story_planning,
            launch_plugin_application,
            discard_plugin_application_presentation,
            handle_plugin_application_bridge_request,
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
        app_handle.state::<TerminalToolWorker>().reset().await;
        app_handle.state::<ProjectRunManager>().reset().await;
        app_handle.state::<RemoteConnectionManager>().reset().await;
        app_handle.state::<LocalTerminalRegistry>().reset().await;
        app_handle.state::<MediaStudioManager>().reset();
        app_handle.state::<StoryStudioManager>().reset();
        app_handle.state::<PluginApplicationManager>().reset().await;
        app_handle.state::<HostManager>().shutdown().await;
    });
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
