-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE notepad_folders (
    user_id TEXT NOT NULL,
    path TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, path)
);

CREATE TABLE notepad_notes (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    folder TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    deleted_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL
);
CREATE INDEX notepad_notes_user_updated_idx
    ON notepad_notes(user_id, updated_at DESC, id)
    WHERE deleted_at IS NULL;
CREATE INDEX notepad_notes_user_folder_idx
    ON notepad_notes(user_id, folder, updated_at DESC)
    WHERE deleted_at IS NULL;

CREATE TABLE notepad_note_revisions (
    revision_id BIGSERIAL PRIMARY KEY,
    note_id TEXT NOT NULL REFERENCES notepad_notes(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('created', 'updated', 'deleted', 'imported')),
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    UNIQUE (note_id, version)
);
CREATE INDEX notepad_note_revisions_user_note_idx
    ON notepad_note_revisions(user_id, note_id, version DESC);

CREATE TABLE notepad_storage_migrations (
    user_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    imported_notes BIGINT NOT NULL,
    imported_folders BIGINT NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL
);
