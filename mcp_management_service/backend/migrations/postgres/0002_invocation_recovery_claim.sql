-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE mcp_management_runtime_invocations
    ADD COLUMN recovery_claim_token TEXT NULL,
    ADD COLUMN recovery_claim_until TIMESTAMPTZ NULL;

CREATE INDEX mcp_runtime_invocations_expired_recovery_idx
    ON mcp_management_runtime_invocations(expires_at, recovery_claim_until, invocation_id)
    WHERE status IN ('queued', 'running', 'waiting_for_user', 'cancel_requested');
