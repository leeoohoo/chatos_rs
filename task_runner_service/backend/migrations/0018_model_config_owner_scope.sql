ALTER TABLE model_configs ADD COLUMN owner_user_id TEXT;

CREATE INDEX IF NOT EXISTS idx_model_configs_owner_user_id
ON model_configs(owner_user_id);
