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
