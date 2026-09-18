-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE OR REPLACE FUNCTION task_tags_search_text(value TEXT[])
RETURNS TEXT
LANGUAGE SQL
IMMUTABLE
PARALLEL SAFE
AS $$
    SELECT array_to_string(COALESCE(value, '{}'::text[]), U&'\001F')
$$;

DROP INDEX IF EXISTS tasks_status_updated_idx;
DROP INDEX IF EXISTS tasks_owner_project_idx;

CREATE INDEX tasks_updated_cursor_idx ON tasks(updated_at DESC,id);
CREATE INDEX tasks_status_updated_idx ON tasks(status,updated_at DESC,id);
CREATE INDEX tasks_owner_project_idx ON tasks(owner_user_id,project_id,updated_at DESC,id);
CREATE INDEX tasks_effective_owner_updated_idx
    ON tasks((coalesce(nullif(btrim(owner_user_id),''),creator_user_id)),updated_at DESC,id);

CREATE INDEX tasks_id_trgm_idx
    ON tasks USING GIN (lower(id) gin_trgm_ops);
CREATE INDEX tasks_title_trgm_idx
    ON tasks USING GIN (lower(coalesce(data->>'title','')) gin_trgm_ops);
CREATE INDEX tasks_objective_trgm_idx
    ON tasks USING GIN (lower(coalesce(data->>'objective','')) gin_trgm_ops);
CREATE INDEX tasks_description_trgm_idx
    ON tasks USING GIN (lower(coalesce(data->>'description','')) gin_trgm_ops);
CREATE INDEX tasks_result_summary_trgm_idx
    ON tasks USING GIN (lower(coalesce(data->>'result_summary','')) gin_trgm_ops);
CREATE INDEX tasks_tags_search_trgm_idx
    ON tasks USING GIN (lower(task_tags_search_text(tags)) gin_trgm_ops);
