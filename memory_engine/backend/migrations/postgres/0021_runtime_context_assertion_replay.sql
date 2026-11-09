-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE engine_runtime_context_assertion_replays (
    jti_hash    TEXT        PRIMARY KEY CHECK (jti_hash LIKE 'sha256:%'),
    scope_hash  TEXT        NOT NULL CHECK (scope_hash LIKE 'sha256:%'),
    issuer      TEXT        NOT NULL,
    key_id      TEXT        NOT NULL,
    operation   TEXT        NOT NULL CHECK (operation IN (
        'record_ingest','compose','summarize','repair'
    )),
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX engine_runtime_context_assertion_replays_expiry_idx
    ON engine_runtime_context_assertion_replays(expires_at);

REVOKE ALL ON engine_runtime_context_assertion_replays FROM PUBLIC;
