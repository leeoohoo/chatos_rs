-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE engine_reliability_operator_actions
    DROP CONSTRAINT engine_reliability_operator_actions_object_kind_check;

ALTER TABLE engine_reliability_operator_actions
    ADD CONSTRAINT engine_reliability_operator_actions_object_kind_check
    CHECK (object_kind IN (
        'record','thread_summary','subject_memory','thread_partition','rebuild_backlog',
        'reliability_queue_item'
    ));
