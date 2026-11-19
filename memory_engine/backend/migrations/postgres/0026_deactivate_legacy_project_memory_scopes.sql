-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

-- Identity v2 replaced project_memory:* scopes with identity:project:* scopes.
-- Keeping both active makes them process the same summaries under different scope keys.
UPDATE engine_subject_memory_scopes legacy
SET status='inactive',
    subject_memory_status='idle',
    subject_memory_dispatch_pending=false,
    subject_memory_dispatch_consumed_version=GREATEST(
        legacy.subject_memory_dispatch_consumed_version,
        legacy.subject_memory_dispatch_version
    ),
    subject_memory_dispatch_consumed_at=now(),
    lock_owner=NULL,
    lock_expires_at=NULL,
    updated_at=now(),
    data=jsonb_set(
        jsonb_set(legacy.data,'{status}',to_jsonb('inactive'::text),true),
        '{updated_at}',
        to_jsonb(to_char(CURRENT_TIMESTAMP AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')),
        true
    )
WHERE legacy.status='active'
  AND legacy.memory_type='project_memory'
  AND legacy.scope_key='project_memory:' || legacy.subject_id
  AND EXISTS (
      SELECT 1
      FROM engine_subject_memory_scopes canonical
      WHERE canonical.tenant_id=legacy.tenant_id
        AND canonical.source_id=legacy.source_id
        AND canonical.subject_id=legacy.subject_id
        AND canonical.scope_key='identity:' || legacy.subject_id
        AND canonical.memory_type='project_summary'
        AND canonical.status='active'
  );
