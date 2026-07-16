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
