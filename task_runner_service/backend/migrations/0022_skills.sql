CREATE TABLE IF NOT EXISTS skills (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  display_name TEXT NOT NULL,
  description TEXT,
  content TEXT NOT NULL,
  locale TEXT NOT NULL DEFAULT 'zh-CN',
  tags_json TEXT NOT NULL DEFAULT '[]',
  source TEXT NOT NULL DEFAULT 'manual',
  source_url TEXT,
  source_registry TEXT,
  source_package_id TEXT,
  version TEXT,
  checksum TEXT,
  install_status TEXT NOT NULL DEFAULT 'installed',
  enabled INTEGER NOT NULL DEFAULT 1,
  auto_inject INTEGER NOT NULL DEFAULT 0,
  scope TEXT NOT NULL DEFAULT 'user',
  creator_user_id TEXT,
  creator_username TEXT,
  creator_display_name TEXT,
  owner_user_id TEXT,
  owner_username TEXT,
  owner_display_name TEXT,
  installed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skills_owner_user_id
ON skills(owner_user_id);

CREATE INDEX IF NOT EXISTS idx_skills_enabled
ON skills(enabled);

CREATE INDEX IF NOT EXISTS idx_skills_auto_inject
ON skills(auto_inject);

CREATE INDEX IF NOT EXISTS idx_skills_source_package
ON skills(source_registry, source_package_id);

CREATE INDEX IF NOT EXISTS idx_skills_updated_at
ON skills(updated_at DESC);
