-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE task_run_event_outbox (
    event_id TEXT PRIMARY KEY REFERENCES task_run_events(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'publishing', 'published', 'dead_letter')),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    publish_attempts INTEGER NOT NULL DEFAULT 0 CHECK(publish_attempts >= 0),
    claim_token TEXT NULL,
    claim_until TIMESTAMPTZ NULL,
    last_error TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX task_run_event_outbox_ready_idx
    ON task_run_event_outbox(available_at, event_id)
    WHERE status = 'pending' OR status = 'publishing';
