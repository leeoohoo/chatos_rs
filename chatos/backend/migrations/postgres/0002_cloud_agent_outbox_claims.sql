-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE cloud_agent_outbox
    ADD COLUMN claim_token TEXT NULL,
    ADD COLUMN claim_until TIMESTAMPTZ NULL,
    ADD CONSTRAINT cloud_agent_outbox_status_check
        CHECK (status IN ('pending', 'publishing', 'published', 'dead_lettered')),
    ADD CONSTRAINT cloud_agent_outbox_claim_shape_check
        CHECK (
            (status = 'publishing' AND claim_token IS NOT NULL AND claim_until IS NOT NULL)
            OR
            (status <> 'publishing' AND claim_token IS NULL AND claim_until IS NULL)
        );

DROP INDEX cloud_agent_outbox_ready_idx;
CREATE INDEX cloud_agent_outbox_pending_idx
    ON cloud_agent_outbox(available_at, event_id)
    WHERE status = 'pending';
CREATE INDEX cloud_agent_outbox_stale_claim_idx
    ON cloud_agent_outbox(claim_until, event_id)
    WHERE status = 'publishing';
