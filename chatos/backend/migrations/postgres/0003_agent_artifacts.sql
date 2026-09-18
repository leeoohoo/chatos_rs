-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE agent_artifacts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('staged', 'uploaded')),
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
    sha256 TEXT NOT NULL,
    bucket TEXT NOT NULL,
    object_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(user_id, idempotency_key),
    UNIQUE(bucket, object_key)
);

CREATE INDEX agent_artifacts_owner_created_idx
    ON agent_artifacts(user_id, created_at DESC);
CREATE INDEX agent_artifacts_staged_cleanup_idx
    ON agent_artifacts(created_at, id)
    WHERE status = 'staged';
