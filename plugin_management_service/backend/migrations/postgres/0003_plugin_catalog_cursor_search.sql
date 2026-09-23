-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE OR REPLACE FUNCTION plugin_catalog_keywords_search_text(value JSONB)
RETURNS TEXT
LANGUAGE SQL
IMMUTABLE
PARALLEL SAFE
AS $$
    SELECT COALESCE(string_agg(keyword, U&'\001F'), '')
    FROM jsonb_array_elements_text(COALESCE(value, '[]'::jsonb)) AS item(keyword)
$$;

CREATE INDEX plugin_catalog_entries_order_idx
    ON plugin_catalog_entries(featured DESC,category,display_name,id);
CREATE INDEX plugin_catalog_entries_id_trgm_idx
    ON plugin_catalog_entries USING GIN (lower(id) gin_trgm_ops);
CREATE INDEX plugin_catalog_entries_name_trgm_idx
    ON plugin_catalog_entries USING GIN (lower(name) gin_trgm_ops);
CREATE INDEX plugin_catalog_entries_display_name_trgm_idx
    ON plugin_catalog_entries USING GIN (lower(display_name) gin_trgm_ops);
CREATE INDEX plugin_catalog_entries_description_trgm_idx
    ON plugin_catalog_entries USING GIN (lower(data->>'description') gin_trgm_ops);
CREATE INDEX plugin_catalog_entries_keywords_trgm_idx
    ON plugin_catalog_entries USING GIN (
        lower(plugin_catalog_keywords_search_text(data->'keywords')) gin_trgm_ops
    );
