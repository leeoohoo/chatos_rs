-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'agent';

UPDATE users SET role = 'admin' WHERE username = 'admin';

CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);
