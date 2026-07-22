ALTER TABLE sessions
ADD COLUMN contact_id TEXT;

CREATE INDEX IF NOT EXISTS idx_sessions_contact_project
ON sessions(owner_user_id, project_id, contact_id, updated_at DESC);
