ALTER TABLE runtime_settings
ADD COLUMN execution_environment_mode TEXT NOT NULL DEFAULT 'local';

ALTER TABLE runtime_settings
ADD COLUMN sandbox_manager_base_url TEXT NOT NULL DEFAULT 'http://127.0.0.1:8095';

ALTER TABLE runtime_settings
ADD COLUMN sandbox_lease_ttl_seconds INTEGER NOT NULL DEFAULT 7200;
