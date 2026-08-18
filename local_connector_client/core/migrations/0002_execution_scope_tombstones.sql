CREATE TABLE IF NOT EXISTS execution_scope_tombstones (
    owner_user_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    terminal_status TEXT NOT NULL,
    finalized_at_unix INTEGER NOT NULL,
    expires_at_unix INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, project_id, run_id, generation)
);

CREATE INDEX IF NOT EXISTS idx_execution_scope_tombstones_expiry
ON execution_scope_tombstones(expires_at_unix);
