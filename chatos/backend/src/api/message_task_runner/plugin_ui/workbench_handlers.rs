// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn get_plugin_ui_asset(
    auth: AuthUser,
    Path((message_id, run_id, event_id, asset_path)): Path<(String, String, String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> Result<Response, ApiError> {
    let relative_path = normalize_requested_asset_path(asset_path.as_str())?;
    let ready = resolve_ready_event_for_message(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        &query,
    )
    .await?;
    let asset = request_local_connector_asset(&auth, &ready, relative_path.as_str()).await?;
    validate_asset_response(&auth, &ready, relative_path.as_str(), &asset)?;
    plugin_ui_asset_response(&ready.ui, asset, None)
}

pub(super) async fn create_plugin_ui_workbench_session(
    auth: AuthUser,
    Path((message_id, run_id, event_id)): Path<(String, String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> Result<Response, ApiError> {
    let ready = resolve_ready_event_for_message(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        &query,
    )
    .await?;
    let config = Config::try_get().map_err(|_| service_unavailable("ChatOS 配置不可用"))?;
    let issued = issue_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        event_id.as_str(),
        ready,
        config.plugin_ui_resource_origin.as_deref(),
    )?;
    let mut response = Json(issued).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    Ok(response)
}

pub(super) async fn revoke_plugin_ui_workbench_session(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id)): Path<(String, String, String, String)>,
) -> Result<StatusCode, ApiError> {
    let session_id = normalize_workbench_session_id(session_id.as_str())?;
    let mut sessions = lock_workbench_sessions()?;
    prune_expired_workbench_sessions(&mut sessions);
    let Some(session) = sessions.get(session_id) else {
        return Err(not_found("Plugin UI Workbench session 不存在"));
    };
    if session.owner_user_id != auth.user_id
        || session.message_id != message_id
        || session.run_id != run_id
        || session.event_id != event_id
    {
        return Err(not_found("Plugin UI Workbench session 不存在"));
    }
    sessions.remove(session_id);
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_plugin_ui_workbench_artifacts(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id)): Path<(String, String, String, String)>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    let access = artifact_access_for_session(&session);
    let response =
        request_local_connector_artifact_list(&auth, &session.ready, access.clone()).await?;
    validate_artifact_list_response(&auth, &session.ready, &access, &response)?;
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub(super) async fn create_plugin_ui_workbench_artifact(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id)): Path<(String, String, String, String)>,
    Json(body): Json<PluginUiWorkbenchArtifactCreateBody>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    require_workbench_capability(&session, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE)?;
    let bytes = decode_artifact_write_body(body.body_base64.as_str())?;
    let access = artifact_access_for_session(&session);
    let response = request_local_connector_artifact_create(
        &auth,
        &session.ready,
        access.clone(),
        body.display_name.as_str(),
        body.media_type.as_str(),
        body.body_base64,
    )
    .await?;
    validate_artifact_write_response(
        &auth,
        &session.ready,
        &access,
        PluginArtifactWriteOperation::Create,
        None,
        Some((body.display_name.as_str(), body.media_type.as_str())),
        bytes.as_slice(),
        &response,
    )?;
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub(super) async fn read_plugin_ui_workbench_artifact(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id, artifact_id)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    let artifact_id = normalize_plugin_artifact_id(artifact_id.as_str())?;
    let access = artifact_access_for_session(&session);
    let response = request_local_connector_artifact_read(
        &auth,
        &session.ready,
        access.clone(),
        artifact_id,
        PluginArtifactReadMode::Inline,
    )
    .await?;
    validate_artifact_read_response(&auth, &session.ready, &access, artifact_id, &response)?;
    if response.artifact.size_bytes > PLUGIN_ARTIFACT_INLINE_READ_MAX_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error": "Plugin Artifact 过大，无法内联读取"})),
        ));
    }
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub(super) async fn update_plugin_ui_workbench_artifact(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id, artifact_id)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
    Json(body): Json<PluginUiWorkbenchArtifactUpdateBody>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    require_workbench_capability(&session, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE)?;
    let artifact_id = normalize_plugin_artifact_id(artifact_id.as_str())?;
    if !is_lower_sha256(body.expected_sha256.as_str()) {
        return Err(bad_request("expected_sha256 无效"));
    }
    let bytes = decode_artifact_write_body(body.body_base64.as_str())?;
    let access = artifact_access_for_session(&session);
    let response = request_local_connector_artifact_update(
        &auth,
        &session.ready,
        access.clone(),
        artifact_id,
        body.expected_sha256.as_str(),
        body.body_base64,
    )
    .await?;
    validate_artifact_write_response(
        &auth,
        &session.ready,
        &access,
        PluginArtifactWriteOperation::Update,
        Some(artifact_id),
        None,
        bytes.as_slice(),
        &response,
    )?;
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub(super) async fn download_plugin_ui_workbench_artifact(
    Path((session_id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let session = get_plugin_ui_workbench_session(session_id.as_str())?;
    let artifact_id = normalize_plugin_artifact_id(artifact_id.as_str())?;
    let auth = AuthUser {
        user_id: session.owner_user_id.clone(),
        role: "user".to_string(),
    };
    let access = artifact_access_for_session(&session);
    let artifact = request_local_connector_artifact_read(
        &auth,
        &session.ready,
        access.clone(),
        artifact_id,
        PluginArtifactReadMode::Download,
    )
    .await?;
    validate_artifact_read_response(&auth, &session.ready, &access, artifact_id, &artifact)?;
    get_plugin_ui_workbench_session(session_id.as_str())?;
    plugin_artifact_download_response(artifact)
}

pub(super) async fn get_plugin_ui_workbench_asset(
    Path((session_id, asset_path)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let relative_path = normalize_requested_asset_path(asset_path.as_str())?;
    let session = get_plugin_ui_workbench_session(session_id.as_str())?;
    let auth = AuthUser {
        user_id: session.owner_user_id,
        role: "user".to_string(),
    };
    let asset =
        request_local_connector_asset(&auth, &session.ready, relative_path.as_str()).await?;
    validate_asset_response(&auth, &session.ready, relative_path.as_str(), &asset)?;
    get_plugin_ui_workbench_session(session_id.as_str())?;
    let config = Config::try_get().map_err(|_| service_unavailable("ChatOS 配置不可用"))?;
    plugin_ui_asset_response(
        &session.ready.ui,
        asset,
        config.plugin_ui_parent_origin.as_deref(),
    )
}
