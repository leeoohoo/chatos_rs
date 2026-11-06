-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE INDEX IF NOT EXISTS engine_records_conversation_cursor_idx
ON engine_records(
  tenant_id,
  source_id,
  thread_id,
  ((data->'metadata'->>'conversation_id')),
  created_at,
  id
);
