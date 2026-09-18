-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

DROP INDEX IF EXISTS plugin_catalog_entries_order_idx;

-- NOT featured converts the mixed DESC/ASC order into one ascending row value,
-- allowing PostgreSQL to seek directly from the complete cursor tuple.
CREATE INDEX plugin_catalog_entries_order_idx
    ON plugin_catalog_entries((NOT featured),category,display_name,id);
