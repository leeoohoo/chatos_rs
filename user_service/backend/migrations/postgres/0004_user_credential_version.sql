-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE users
    ADD COLUMN credential_version BIGINT NOT NULL DEFAULT 0;

UPDATE users
SET data = jsonb_set(data, '{credential_version}', to_jsonb(credential_version), true);
