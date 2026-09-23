-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

DROP INDEX IF EXISTS plugin_audit_logs_plugin_created_idx;
DROP INDEX IF EXISTS plugin_audit_logs_owner_device_created_idx;

CREATE INDEX plugin_audit_logs_created_cursor_idx
    ON plugin_audit_logs(created_at DESC,id DESC);
CREATE INDEX plugin_audit_logs_plugin_created_idx
    ON plugin_audit_logs(plugin_id,created_at DESC,id DESC);
CREATE INDEX plugin_audit_logs_owner_created_idx
    ON plugin_audit_logs(owner_user_id,created_at DESC,id DESC);
CREATE INDEX plugin_audit_logs_owner_device_created_idx
    ON plugin_audit_logs(owner_user_id,device_id,created_at DESC,id DESC);
