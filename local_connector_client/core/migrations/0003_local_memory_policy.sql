ALTER TABLE session_runtime_settings
ADD COLUMN memory_auto_summary_enabled INTEGER NOT NULL DEFAULT 1;

ALTER TABLE session_runtime_settings
ADD COLUMN memory_summary_message_threshold INTEGER NOT NULL DEFAULT 24;

ALTER TABLE session_runtime_settings
ADD COLUMN memory_summary_character_threshold INTEGER NOT NULL DEFAULT 32000;
