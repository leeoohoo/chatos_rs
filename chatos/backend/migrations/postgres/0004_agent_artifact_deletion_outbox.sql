-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE agent_artifacts
    DROP CONSTRAINT agent_artifacts_status_check;

ALTER TABLE agent_artifacts
    ADD CONSTRAINT agent_artifacts_status_check
    CHECK (status IN ('staged', 'uploaded', 'deleting'));

CREATE TABLE agent_artifact_deletion_outbox (
    artifact_id TEXT PRIMARY KEY REFERENCES agent_artifacts(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX agent_artifact_deletion_outbox_due_idx
    ON agent_artifact_deletion_outbox(next_attempt_at, artifact_id);

CREATE INDEX agent_artifact_deletion_outbox_owner_idx
    ON agent_artifact_deletion_outbox(user_id, created_at, artifact_id);
