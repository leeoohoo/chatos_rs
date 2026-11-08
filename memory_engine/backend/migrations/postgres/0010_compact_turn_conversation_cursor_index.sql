-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE INDEX engine_compact_turns_conversation_page_idx
ON engine_compact_turns(
    tenant_id,
    source_id,
    thread_id,
    record_type,
    (data #>> '{user_record,metadata,conversation_id}'),
    user_created_at DESC,
    user_record_id DESC
);
