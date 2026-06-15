use crate::models::{now_rfc3339, RemoteServerRecord};

use super::TaskRunnerRemoteConnectionStore;

pub(super) async fn resolve_enabled_server(
    store: &TaskRunnerRemoteConnectionStore,
    connection_id: &str,
) -> Result<RemoteServerRecord, String> {
    let server = store
        .store
        .get_remote_server(connection_id)
        .await?
        .ok_or_else(|| format!("远程服务器不存在: {connection_id}"))?;
    if !server.enabled {
        return Err(format!("远程服务器已禁用: {connection_id}"));
    }
    Ok(server)
}

pub(super) async fn touch_server(
    store: &TaskRunnerRemoteConnectionStore,
    connection_id: &str,
) -> Result<(), String> {
    let Some(mut server) = store.store.get_remote_server(connection_id).await? else {
        return Ok(());
    };
    server.last_active_at = Some(now_rfc3339());
    server.updated_at = now_rfc3339();
    store.store.save_remote_server(server).await?;
    Ok(())
}

pub(super) async fn persist_test_result(
    store: &TaskRunnerRemoteConnectionStore,
    connection_id: &str,
    ok: bool,
    message: Option<String>,
) -> Result<(), String> {
    let Some(mut server) = store.store.get_remote_server(connection_id).await? else {
        return Ok(());
    };
    let now = now_rfc3339();
    server.last_tested_at = Some(now.clone());
    server.last_test_status = Some(if ok { "success" } else { "failed" }.to_string());
    server.last_test_message = message;
    server.updated_at = now;
    store.store.save_remote_server(server).await?;
    Ok(())
}
