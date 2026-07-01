-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE runtime_settings
ADD COLUMN sandbox_enabled INTEGER NOT NULL DEFAULT 0;
