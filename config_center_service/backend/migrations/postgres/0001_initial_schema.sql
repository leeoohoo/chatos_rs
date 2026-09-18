-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE config_definitions (
    key TEXT PRIMARY KEY,
    ui_order INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);

CREATE TABLE config_drafts (
    environment TEXT PRIMARY KEY,
    base_revision BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);

CREATE TABLE config_releases (
    id TEXT PRIMARY KEY,
    environment TEXT NOT NULL,
    revision BIGINT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ NULL,
    data JSONB NOT NULL,
    CONSTRAINT config_releases_environment_revision_unique UNIQUE (environment, revision)
);

CREATE INDEX config_releases_environment_revision_idx
    ON config_releases (environment, revision DESC);

CREATE TABLE config_snapshots (
    environment TEXT NOT NULL,
    service_name TEXT NOT NULL,
    revision BIGINT NOT NULL,
    checksum TEXT NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    PRIMARY KEY (environment, service_name, revision)
);

CREATE TABLE config_active_releases (
    environment TEXT PRIMARY KEY,
    release_id TEXT NOT NULL REFERENCES config_releases(id) ON DELETE RESTRICT,
    revision BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);

CREATE TABLE config_audit_events (
    id TEXT PRIMARY KEY,
    environment TEXT NULL,
    action TEXT NOT NULL,
    actor_user_id TEXT NOT NULL,
    release_id TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);

CREATE INDEX config_audit_events_created_at_idx
    ON config_audit_events (created_at DESC, id);

CREATE TABLE config_service_instances (
    id TEXT PRIMARY KEY,
    environment TEXT NOT NULL,
    service_name TEXT NOT NULL,
    service_id TEXT NOT NULL,
    effective_revision BIGINT NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL,
    CONSTRAINT config_service_instances_identity_unique
        UNIQUE (environment, service_name, service_id)
);

CREATE INDEX config_service_instances_listing_idx
    ON config_service_instances (environment, service_name, service_id);

CREATE TABLE config_platform_pressure_states (
    environment TEXT PRIMARY KEY,
    level TEXT NOT NULL CHECK (level IN ('normal', 'elevated', 'critical')),
    updated_at TIMESTAMPTZ NOT NULL,
    data JSONB NOT NULL
);
