-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX task_runs_id_trgm_idx
    ON task_runs USING GIN (lower(id) gin_trgm_ops);
CREATE INDEX task_runs_task_id_trgm_idx
    ON task_runs USING GIN (lower(task_id) gin_trgm_ops);
CREATE INDEX task_runs_model_config_id_trgm_idx
    ON task_runs USING GIN (lower(model_config_id) gin_trgm_ops);
CREATE INDEX task_runs_result_summary_trgm_idx
    ON task_runs USING GIN (lower(coalesce(data->>'result_summary', '')) gin_trgm_ops);
CREATE INDEX task_runs_error_message_trgm_idx
    ON task_runs USING GIN (lower(coalesce(data->>'error_message', '')) gin_trgm_ops);
