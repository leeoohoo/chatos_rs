-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE INDEX IF NOT EXISTS engine_records_project_conversation_cursor_idx
ON engine_records(
  tenant_id,
  source_id,
  thread_id,
  (BTRIM(COALESCE(data #>> '{metadata,memory_project_id}', ''))),
  (BTRIM(COALESCE(data #>> '{metadata,conversation_id}', ''))),
  created_at,
  id
);

UPDATE engine_subject_memory_scopes
SET data = data || jsonb_build_object(
      'memory_metadata', CASE
        WHEN jsonb_typeof(data->'memory_metadata') = 'object' THEN data->'memory_metadata'
        ELSE '{}'::jsonb
      END ||
        jsonb_build_object('generation_profile', 'agent_stable_cross_project_v1'),
      'updated_at', to_char(CURRENT_TIMESTAMP AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')
    ),
    updated_at = CURRENT_TIMESTAMP
WHERE memory_type = 'agent_memory'
  AND status = 'active'
  AND BTRIM(COALESCE(data #>> '{memory_metadata,generation_profile}', '')) = '';

-- Legacy Agent memories were generated before project facts and transient Task outcomes were
-- excluded. Keep the rows for audit/recovery, but remove them from active cross-project recall.
UPDATE engine_subject_memories
SET status = 'deleted',
    data = data || jsonb_build_object(
      'status', 'deleted',
      'metadata', CASE
        WHEN jsonb_typeof(data->'metadata') = 'object' THEN data->'metadata'
        ELSE '{}'::jsonb
      END || jsonb_build_object(
        'retired_reason', 'legacy_agent_memory_without_stable_cross_project_profile',
        'retired_at', to_char(CURRENT_TIMESTAMP AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')
      ),
      'updated_at', to_char(CURRENT_TIMESTAMP AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')
    ),
    updated_at = CURRENT_TIMESTAMP
WHERE memory_type = 'agent_memory'
  AND status = 'active'
  AND BTRIM(COALESCE(data #>> '{metadata,generation_profile}', '')) = '';
