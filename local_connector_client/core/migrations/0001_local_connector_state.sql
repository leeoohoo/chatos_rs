CREATE TABLE IF NOT EXISTS agent_capability_snapshots (
    owner_user_id TEXT NOT NULL,
    agent_key TEXT NOT NULL,
    policy_revision TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    synced_at TEXT NOT NULL,
    PRIMARY KEY(owner_user_id, agent_key)
);

CREATE INDEX IF NOT EXISTS idx_agent_capability_snapshots_synced
ON agent_capability_snapshots(owner_user_id, synced_at DESC);

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

CREATE TABLE IF NOT EXISTS system_agent_prompts (
    agent_key TEXT NOT NULL,
    vendor TEXT NOT NULL,
    content TEXT NOT NULL,
    revision INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    bundle_version INTEGER NOT NULL,
    published_at TEXT NOT NULL,
    synced_at TEXT NOT NULL,
    source_instance_id TEXT NOT NULL,
    PRIMARY KEY(agent_key, vendor)
);

CREATE INDEX IF NOT EXISTS idx_system_agent_prompts_bundle
ON system_agent_prompts(source_instance_id, bundle_version);

CREATE TABLE IF NOT EXISTS system_agent_prompt_sync (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    source_instance_id TEXT NOT NULL,
    installed_bundle_version INTEGER NOT NULL DEFAULT 0,
    remote_bundle_version INTEGER NOT NULL DEFAULT 0,
    update_available INTEGER NOT NULL DEFAULT 0,
    required INTEGER NOT NULL DEFAULT 0,
    prompt_count INTEGER NOT NULL DEFAULT 0,
    last_checked_at TEXT,
    last_synced_at TEXT,
    last_error TEXT
);
