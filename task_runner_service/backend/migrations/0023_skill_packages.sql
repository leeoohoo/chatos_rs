-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

ALTER TABLE skills ADD COLUMN package_root TEXT;
ALTER TABLE skills ADD COLUMN package_manifest_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE skills ADD COLUMN package_file_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE skills ADD COLUMN package_total_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE skills ADD COLUMN source_repo TEXT;
ALTER TABLE skills ADD COLUMN source_ref TEXT;
ALTER TABLE skills ADD COLUMN source_path TEXT;
