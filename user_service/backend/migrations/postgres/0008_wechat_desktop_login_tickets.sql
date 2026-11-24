CREATE TABLE wechat_desktop_login_tickets (
    id TEXT PRIMARY KEY,
    ticket_hash TEXT NOT NULL UNIQUE,
    poll_secret_hash TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    app_id TEXT NOT NULL,
    status TEXT NOT NULL,
    user_id TEXT NULL REFERENCES users(id) ON DELETE RESTRICT,
    expires_at BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);

CREATE INDEX wechat_desktop_login_tickets_expires_idx
    ON wechat_desktop_login_tickets (expires_at);
CREATE INDEX wechat_desktop_login_tickets_source_idx
    ON wechat_desktop_login_tickets (source_hash, updated_at DESC);
