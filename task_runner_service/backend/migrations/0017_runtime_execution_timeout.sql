-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE runtime_settings
  ADD COLUMN execution_timeout_ms INTEGER DEFAULT 7200000;
