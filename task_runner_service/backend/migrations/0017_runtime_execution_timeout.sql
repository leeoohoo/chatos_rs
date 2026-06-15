ALTER TABLE runtime_settings
  ADD COLUMN execution_timeout_ms INTEGER DEFAULT 7200000;
