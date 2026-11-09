-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE engine_privileged_erasure_audits (
    erasure_id             TEXT        PRIMARY KEY,
    erasure_kind           TEXT        NOT NULL CHECK (erasure_kind = 'thread'),
    reason_code            TEXT        NOT NULL CHECK (reason_code IN (
        'user_delete','tenant_delete','legal_erasure','retention_expired'
    )),
    deleted_edges          BIGINT      NOT NULL CHECK (deleted_edges >= 0),
    deleted_coverage       BIGINT      NOT NULL CHECK (deleted_coverage >= 0),
    deleted_summaries      BIGINT      NOT NULL CHECK (deleted_summaries >= 0),
    deleted_memories       BIGINT      NOT NULL CHECK (deleted_memories >= 0),
    deleted_records        BIGINT      NOT NULL CHECK (deleted_records >= 0),
    deleted_projections    BIGINT      NOT NULL CHECK (deleted_projections >= 0),
    deleted_generations    BIGINT      NOT NULL CHECK (deleted_generations >= 0),
    deleted_jobs           BIGINT      NOT NULL CHECK (deleted_jobs >= 0),
    deleted_subjects       BIGINT      NOT NULL CHECK (deleted_subjects >= 0),
    deleted_thread         BIGINT      NOT NULL CHECK (deleted_thread IN (0,1)),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

REVOKE ALL ON engine_privileged_erasure_audits FROM PUBLIC;
