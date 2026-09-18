-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    last_login_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL
);
CREATE INDEX users_enabled_idx ON users (enabled);
CREATE INDEX users_role_idx ON users (role);
CREATE INDEX users_updated_idx ON users (updated_at DESC, created_at DESC, id);

CREATE TABLE agent_accounts (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    owner_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    enabled BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    last_login_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL
);
CREATE INDEX agent_accounts_owner_idx ON agent_accounts (owner_user_id);
CREATE INDEX agent_accounts_updated_idx ON agent_accounts (updated_at DESC, created_at DESC, id);

CREATE TABLE revoked_tokens (
    jti TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL,
    revoked_at TIMESTAMPTZ NOT NULL,
    expires_at BIGINT NOT NULL
);
CREATE INDEX revoked_tokens_expires_at_idx ON revoked_tokens (expires_at);

CREATE TABLE user_model_providers (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    updated_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX user_model_providers_owner_updated_idx
    ON user_model_providers (owner_user_id, updated_at DESC, created_at DESC, id);

CREATE TABLE user_model_configs (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    source_provider_id TEXT NULL REFERENCES user_model_providers(id) ON DELETE SET NULL,
    enabled BOOLEAN NOT NULL,
    task_enabled BOOLEAN NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX user_model_configs_owner_updated_idx
    ON user_model_configs (owner_user_id, updated_at DESC, created_at DESC, id);

CREATE TABLE user_model_settings (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE RESTRICT,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);

CREATE TABLE harness_provisioning (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE RESTRICT,
    status TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX harness_provisioning_status_idx ON harness_provisioning (status);

CREATE TABLE registration_email_codes (
    email TEXT PRIMARY KEY,
    expires_at BIGINT NOT NULL,
    consumed_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX registration_email_codes_expires_at_idx ON registration_email_codes (expires_at);

CREATE TABLE invite_codes (
    id TEXT PRIMARY KEY,
    code_hash TEXT NOT NULL UNIQUE,
    created_by_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    max_uses BIGINT NOT NULL CHECK (max_uses >= 0),
    used_count BIGINT NOT NULL CHECK (used_count >= 0),
    expires_at BIGINT NULL,
    revoked_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX invite_codes_created_idx ON invite_codes (created_at DESC, id);
CREATE INDEX invite_codes_expires_idx ON invite_codes (expires_at);

CREATE TABLE local_connector_auth_tickets (
    id TEXT PRIMARY KEY,
    ticket_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    expires_at BIGINT NOT NULL,
    consumed_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX local_connector_auth_tickets_expires_idx
    ON local_connector_auth_tickets (expires_at);

CREATE TABLE user_external_identities (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL,
    app_id TEXT NOT NULL,
    open_id_hash TEXT NOT NULL,
    union_id_hash TEXT NULL,
    revoked_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE UNIQUE INDEX user_external_identities_active_subject_unique
    ON user_external_identities (provider, app_id, open_id_hash)
    WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX user_external_identities_active_user_provider_unique
    ON user_external_identities (user_id, provider, app_id)
    WHERE revoked_at IS NULL;

CREATE TABLE wechat_bind_tickets (
    id TEXT PRIMARY KEY,
    ticket_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    app_id TEXT NOT NULL,
    status TEXT NOT NULL,
    claim_id TEXT NULL,
    expires_at BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE UNIQUE INDEX wechat_bind_tickets_claim_id_unique
    ON wechat_bind_tickets (claim_id) WHERE claim_id IS NOT NULL;
CREATE INDEX wechat_bind_tickets_expires_idx ON wechat_bind_tickets (expires_at);

CREATE TABLE client_sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    client_type TEXT NOT NULL,
    external_identity_id TEXT NULL REFERENCES user_external_identities(id) ON DELETE RESTRICT,
    token_jti TEXT NOT NULL UNIQUE,
    expires_at BIGINT NOT NULL,
    revoked_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
CREATE INDEX client_sessions_user_type_idx ON client_sessions (user_id, client_type);
CREATE INDEX client_sessions_expires_idx ON client_sessions (expires_at);

CREATE TABLE device_proof_nonces (
    id TEXT PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX device_proof_nonces_expires_idx ON device_proof_nonces (expires_at);

CREATE TABLE login_throttle (
    key TEXT PRIMARY KEY,
    attempts BIGINT NOT NULL CHECK (attempts >= 0),
    window_start_unix BIGINT NOT NULL,
    locked_until_unix BIGINT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX login_throttle_expires_idx ON login_throttle (expires_at);
