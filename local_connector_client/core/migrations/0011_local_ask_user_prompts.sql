CREATE TABLE IF NOT EXISTS ask_user_prompts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    tool_call_id TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    prompt_json TEXT NOT NULL,
    response_json TEXT,
    expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(turn_id) REFERENCES turns(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ask_user_prompts_session_status
ON ask_user_prompts(session_id, status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_ask_user_prompts_turn
ON ask_user_prompts(turn_id, created_at ASC);
