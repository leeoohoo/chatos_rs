use super::{
    runtime::{
        PluginApplicationBridgeRequest, PluginApplicationLaunchRequest,
        PresentedPluginApplicationBridgeResponse, PresentedPluginApplicationLaunch,
    },
    PluginApplicationManager, PresentedPluginApplication,
};
use tauri::State;

#[tauri::command]
pub async fn list_plugin_applications(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    host: State<'_, crate::host_lifecycle::HostManager>,
    manager: State<'_, PluginApplicationManager>,
    owner_user_id: String,
) -> Result<Vec<PresentedPluginApplication>, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager.list(&*host, &owner_user_id).await
}

#[tauri::command]
pub async fn launch_plugin_application(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    host: State<'_, crate::host_lifecycle::HostManager>,
    manager: State<'_, PluginApplicationManager>,
    request: PluginApplicationLaunchRequest,
) -> Result<PresentedPluginApplicationLaunch, String> {
    let epoch = manager.runtime.epoch();
    authentication
        .access_token_for_owner(&request.owner_user_id)
        .await?;
    let resolved = manager
        .resolve(
            &*host,
            &request.owner_user_id,
            &request.plugin_id,
            &request.component_key,
        )
        .await?;
    manager.runtime.launch(epoch, request, resolved).await
}

#[tauri::command]
pub async fn discard_plugin_application_presentation(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, PluginApplicationManager>,
    owner_user_id: String,
    presentation_id: String,
    generation: u64,
) -> Result<(), String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager
        .runtime
        .discard_presentation(&owner_user_id, &presentation_id, generation)
        .await
}

#[tauri::command]
pub async fn handle_plugin_application_bridge_request(
    authentication: State<'_, crate::authentication::AuthenticationManager>,
    manager: State<'_, PluginApplicationManager>,
    request: PluginApplicationBridgeRequest,
) -> Result<PresentedPluginApplicationBridgeResponse, String> {
    authentication
        .access_token_for_owner(&request.owner_user_id)
        .await?;
    manager.runtime.bridge_request(request).await
}
