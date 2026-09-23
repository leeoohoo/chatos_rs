-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE task_runner_maintenance_leases (
    lease_name TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    lease_until TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX task_runner_maintenance_leases_expiry_idx
    ON task_runner_maintenance_leases(lease_until, lease_name);
