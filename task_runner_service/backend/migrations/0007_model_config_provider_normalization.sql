-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

PRAGMA foreign_keys = OFF;

CREATE TABLE model_configs_v2 (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL DEFAULT 'openai',
  base_url TEXT NOT NULL,
  api_key TEXT NOT NULL,
  model TEXT NOT NULL,
  temperature REAL,
  max_output_tokens INTEGER,
  thinking_level TEXT,
  supports_responses INTEGER NOT NULL DEFAULT 0,
  instructions TEXT,
  request_cwd TEXT,
  include_prompt_cache_retention INTEGER NOT NULL DEFAULT 0,
  request_body_limit_bytes INTEGER,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

INSERT INTO model_configs_v2 (
  id,
  name,
  provider,
  base_url,
  api_key,
  model,
  temperature,
  max_output_tokens,
  thinking_level,
  supports_responses,
  instructions,
  request_cwd,
  include_prompt_cache_retention,
  request_body_limit_bytes,
  enabled,
  created_at,
  updated_at
)
SELECT
  id,
  name,
  CASE LOWER(TRIM(COALESCE(provider, '')))
    WHEN 'openai_compatible' THEN 'openai'
    WHEN 'openai-compatible' THEN 'openai'
    WHEN 'custom_gateway' THEN 'openai'
    WHEN 'gpt' THEN 'openai'
    WHEN 'kimi' THEN 'kimik2'
    WHEN 'moonshot' THEN 'kimik2'
    WHEN 'kiminik2' THEN 'kimik2'
    ELSE provider
  END AS provider,
  base_url,
  api_key,
  model,
  temperature,
  max_output_tokens,
  thinking_level,
  supports_responses,
  instructions,
  request_cwd,
  include_prompt_cache_retention,
  request_body_limit_bytes,
  enabled,
  created_at,
  updated_at
FROM model_configs;

DROP TABLE model_configs;
ALTER TABLE model_configs_v2 RENAME TO model_configs;
CREATE INDEX IF NOT EXISTS idx_model_configs_updated_at ON model_configs(updated_at DESC);

PRAGMA foreign_keys = ON;
