-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE task_runs ADD COLUMN worker_id TEXT;
ALTER TABLE task_runs ADD COLUMN claim_token TEXT;
ALTER TABLE task_runs ADD COLUMN claim_until TEXT;
ALTER TABLE task_runs ADD COLUMN attempt INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_task_runs_claim_until
  ON task_runs(status, claim_until);

CREATE INDEX IF NOT EXISTS idx_task_runs_worker_claim
  ON task_runs(worker_id, claim_token);
