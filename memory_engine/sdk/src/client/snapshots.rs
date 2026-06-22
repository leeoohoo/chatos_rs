use reqwest::Method;

use crate::models::{
    EngineThreadSnapshot, SdkGetLatestThreadSnapshotRequest, SdkGetThreadSnapshotByTurnRequest,
    SdkUpsertThreadSnapshotRequest, ThreadSnapshotLookupResponse, UpsertThreadSnapshotRequest,
};

use super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn upsert_thread_snapshot(
        &self,
        thread_id: &str,
        snapshot_type: &str,
        turn_id: &str,
        req: &SdkUpsertThreadSnapshotRequest,
    ) -> Result<EngineThreadSnapshot, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id = require_direct_source_id(source_id, "upsert_thread_snapshot")?;
                let direct = UpsertThreadSnapshotRequest {
                    tenant_id: req.tenant_id.clone(),
                    source_id: source_id.to_string(),
                    user_message_id: req.user_message_id.clone(),
                    status: req.status.clone(),
                    snapshot_source: req.snapshot_source.clone(),
                    snapshot_version: req.snapshot_version,
                    payload: req.payload.clone(),
                    metadata: req.metadata.clone(),
                    captured_at: req.captured_at.clone(),
                };
                self.send_json(
                    Method::PUT,
                    &format!(
                        "/threads/{}/snapshots/{}/turns/{}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(snapshot_type),
                        urlencoding::encode(turn_id)
                    ),
                    Some(&direct),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::PUT,
                    &format!(
                        "/sdk/threads/{}/snapshots/{}/turns/{}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(snapshot_type),
                        urlencoding::encode(turn_id)
                    ),
                    Some(req),
                )
                .await
            }
        }
    }

    pub async fn get_latest_thread_snapshot(
        &self,
        thread_id: &str,
        snapshot_type: &str,
        tenant_id: &str,
    ) -> Result<ThreadSnapshotLookupResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "get_latest_thread_snapshot")?;
                self.send_json::<ThreadSnapshotLookupResponse, _>(
                    Method::GET,
                    &format!(
                        "/threads/{}/snapshots/{}/latest?tenant_id={}&source_id={}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(snapshot_type),
                        urlencoding::encode(tenant_id),
                        urlencoding::encode(source_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/snapshots/{}/latest",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(snapshot_type)
                    ),
                    Some(&SdkGetLatestThreadSnapshotRequest {
                        tenant_id: tenant_id.to_string(),
                    }),
                )
                .await
            }
        }
    }

    pub async fn get_thread_snapshot_by_turn(
        &self,
        thread_id: &str,
        snapshot_type: &str,
        turn_id: &str,
        tenant_id: &str,
    ) -> Result<ThreadSnapshotLookupResponse, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "get_thread_snapshot_by_turn")?;
                self.send_json::<ThreadSnapshotLookupResponse, _>(
                    Method::GET,
                    &format!(
                        "/threads/{}/snapshots/{}/turns/{}?tenant_id={}&source_id={}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(snapshot_type),
                        urlencoding::encode(turn_id),
                        urlencoding::encode(tenant_id),
                        urlencoding::encode(source_id)
                    ),
                    Option::<&()>::None,
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::POST,
                    &format!(
                        "/sdk/threads/{}/snapshots/{}/turns/{}",
                        urlencoding::encode(thread_id),
                        urlencoding::encode(snapshot_type),
                        urlencoding::encode(turn_id)
                    ),
                    Some(&SdkGetThreadSnapshotByTurnRequest {
                        tenant_id: tenant_id.to_string(),
                    }),
                )
                .await
            }
        }
    }
}
