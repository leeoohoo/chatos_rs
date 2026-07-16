CREATE TABLE IF NOT EXISTS local_mcp_manifests (
    manifest_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    plugin_mcp_id TEXT,
    enabled INTEGER NOT NULL,
    sync_status TEXT NOT NULL,
    last_check_status TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(owner_user_id, device_id, manifest_id)
);

CREATE INDEX IF NOT EXISTS idx_local_mcp_manifests_owner_device
ON local_mcp_manifests(owner_user_id, device_id, updated_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_local_mcp_manifests_plugin_resource
ON local_mcp_manifests(owner_user_id, device_id, plugin_mcp_id)
WHERE plugin_mcp_id IS NOT NULL;
