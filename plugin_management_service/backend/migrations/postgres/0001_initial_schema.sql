-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE plugin_mcps (
 id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, visibility TEXT NOT NULL,
 source_kind TEXT NOT NULL, name TEXT NOT NULL, display_name TEXT NOT NULL,
 enabled BOOLEAN NOT NULL, runtime_kind TEXT NOT NULL, plugin_id TEXT NULL,
 release_id TEXT NULL, component_key TEXT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_mcps_owner_visibility_enabled_idx ON plugin_mcps(owner_user_id,visibility,enabled);
CREATE INDEX plugin_mcps_visibility_enabled_idx ON plugin_mcps(visibility,enabled);
CREATE INDEX plugin_mcps_runtime_kind_idx ON plugin_mcps(runtime_kind);
CREATE INDEX plugin_mcps_component_idx ON plugin_mcps(plugin_id,release_id,component_key);

CREATE TABLE plugin_skills (
 id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, visibility TEXT NOT NULL,
 source_kind TEXT NOT NULL, name TEXT NOT NULL, display_name TEXT NOT NULL,
 enabled BOOLEAN NOT NULL, content_kind TEXT NOT NULL, plugin_id TEXT NULL,
 release_id TEXT NULL, component_key TEXT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_skills_owner_visibility_enabled_idx ON plugin_skills(owner_user_id,visibility,enabled);
CREATE INDEX plugin_skills_visibility_enabled_idx ON plugin_skills(visibility,enabled);
CREATE INDEX plugin_skills_content_kind_idx ON plugin_skills(content_kind);
CREATE INDEX plugin_skills_component_idx ON plugin_skills(plugin_id,release_id,component_key);

CREATE TABLE plugin_skill_packages (
 id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, visibility TEXT NOT NULL,
 source_kind TEXT NOT NULL, name TEXT NOT NULL, installed BOOLEAN NOT NULL,
 updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_skill_packages_owner_visibility_idx ON plugin_skill_packages(owner_user_id,visibility);

CREATE TABLE plugin_agents (
 agent_key TEXT PRIMARY KEY, service_name TEXT NOT NULL, enabled BOOLEAN NOT NULL,
 plugin_id TEXT NULL, release_id TEXT NULL, component_key TEXT NULL,
 updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_agents_service_enabled_idx ON plugin_agents(service_name,enabled);
CREATE INDEX plugin_agents_component_idx ON plugin_agents(plugin_id,release_id,component_key);

CREATE TABLE plugin_agent_provider_prompts (
 id TEXT PRIMARY KEY, agent_key TEXT NOT NULL, profile TEXT NOT NULL, vendor TEXT NOT NULL,
 enabled BOOLEAN NOT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL,
 UNIQUE(agent_key,profile,vendor)
);
CREATE INDEX plugin_agent_provider_prompts_agent_enabled_idx ON plugin_agent_provider_prompts(agent_key,enabled);

CREATE TABLE plugin_agent_prompt_versions (
 id TEXT PRIMARY KEY, version BIGINT NOT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);

CREATE TABLE plugin_agent_prompt_releases (
 id TEXT PRIMARY KEY, agent_key TEXT NOT NULL, bundle_version BIGINT NOT NULL,
 published_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL, UNIQUE(agent_key,bundle_version)
);

CREATE TABLE plugin_agent_bindings (
 id TEXT PRIMARY KEY, agent_key TEXT NOT NULL, binding_scope TEXT NOT NULL,
 owner_user_id TEXT NULL, resource_kind TEXT NOT NULL, resource_id TEXT NOT NULL,
 enabled BOOLEAN NOT NULL, priority BIGINT NOT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_agent_bindings_agent_scope_owner_idx ON plugin_agent_bindings(agent_key,binding_scope,owner_user_id);
CREATE INDEX plugin_agent_bindings_resource_idx ON plugin_agent_bindings(resource_kind,resource_id);

CREATE TABLE plugin_resource_checks (
 id TEXT PRIMARY KEY, resource_kind TEXT NOT NULL, resource_id TEXT NOT NULL,
 owner_user_id TEXT NOT NULL, status TEXT NOT NULL, last_checked_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_resource_checks_resource_idx ON plugin_resource_checks(resource_kind,resource_id);

CREATE TABLE plugin_marketplaces (
 id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, owner_user_id TEXT NULL,
 visibility TEXT NOT NULL, source_kind TEXT NOT NULL, catalog_url TEXT NULL,
 enabled BOOLEAN NOT NULL, trust_level TEXT NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_marketplaces_lookup_idx ON plugin_marketplaces(visibility,owner_user_id,enabled,trust_level);

CREATE TABLE plugin_catalog_sync_outbox (
 marketplace_id TEXT PRIMARY KEY REFERENCES plugin_marketplaces(id) ON DELETE CASCADE,
 event_version BIGINT NOT NULL, consumed_version BIGINT NOT NULL DEFAULT 0,
 pending BOOLEAN NOT NULL, scheduled BOOLEAN NOT NULL,
 requested_at TIMESTAMPTZ NOT NULL, published_version BIGINT NULL,
 dead_letter_version BIGINT NULL, dead_lettered_at TIMESTAMPTZ NULL,
 last_error TEXT NULL
);
CREATE INDEX plugin_catalog_sync_outbox_pending_idx ON plugin_catalog_sync_outbox(pending,requested_at,marketplace_id);

CREATE TABLE plugin_catalog_sync_locks (
 marketplace_id TEXT PRIMARY KEY REFERENCES plugin_marketplaces(id) ON DELETE CASCADE,
 lock_owner TEXT NOT NULL, lock_until TIMESTAMPTZ NOT NULL
);
CREATE INDEX plugin_catalog_sync_locks_until_idx ON plugin_catalog_sync_locks(lock_until);

CREATE TABLE plugin_publishers (
 id TEXT PRIMARY KEY, marketplace_id TEXT NOT NULL REFERENCES plugin_marketplaces(id) ON DELETE RESTRICT,
 publisher_id TEXT NOT NULL, owner_user_id TEXT NOT NULL, status TEXT NOT NULL,
 updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL, UNIQUE(marketplace_id,publisher_id)
);
CREATE INDEX plugin_publishers_owner_status_updated_idx ON plugin_publishers(owner_user_id,status,updated_at DESC);
CREATE INDEX plugin_publishers_marketplace_status_updated_idx ON plugin_publishers(marketplace_id,status,updated_at DESC);

CREATE TABLE plugin_catalog_syncs (
 marketplace_id TEXT PRIMARY KEY REFERENCES plugin_marketplaces(id) ON DELETE CASCADE,
 synced_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);

CREATE TABLE plugin_catalog_entries (
 id TEXT PRIMARY KEY, plugin_key TEXT NOT NULL UNIQUE,
 marketplace_id TEXT NOT NULL REFERENCES plugin_marketplaces(id) ON DELETE RESTRICT,
 owner_user_id TEXT NULL, name TEXT NOT NULL, display_name TEXT NOT NULL,
 category TEXT NOT NULL, visibility TEXT NOT NULL, enabled BOOLEAN NOT NULL,
 featured BOOLEAN NOT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL,
 UNIQUE(marketplace_id,name)
);
CREATE INDEX plugin_catalog_entries_listing_idx ON plugin_catalog_entries(visibility,owner_user_id,enabled,featured DESC);
CREATE INDEX plugin_catalog_entries_category_name_idx ON plugin_catalog_entries(category,display_name);

CREATE TABLE plugin_releases (
 id TEXT PRIMARY KEY, plugin_id TEXT NOT NULL REFERENCES plugin_catalog_entries(id) ON DELETE RESTRICT,
 version TEXT NOT NULL, release_channel TEXT NOT NULL, published_at TIMESTAMPTZ NOT NULL,
 revoked_at TIMESTAMPTZ NULL, data JSONB NOT NULL, UNIQUE(plugin_id,version)
);
CREATE INDEX plugin_releases_plugin_published_idx ON plugin_releases(plugin_id,published_at DESC,revoked_at);

CREATE TABLE plugin_release_publication_states (
 release_id TEXT PRIMARY KEY REFERENCES plugin_releases(id) ON DELETE CASCADE,
 ready BOOLEAN NOT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_release_publication_states_ready_idx ON plugin_release_publication_states(ready,updated_at DESC);

CREATE TABLE plugin_installations (
 id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, device_id TEXT NOT NULL,
 plugin_id TEXT NOT NULL REFERENCES plugin_catalog_entries(id) ON DELETE RESTRICT,
 release_id TEXT NOT NULL REFERENCES plugin_releases(id) ON DELETE RESTRICT,
 active BOOLEAN NOT NULL, last_checked_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL,
 UNIQUE(owner_user_id,device_id,plugin_id)
);
CREATE INDEX plugin_installations_owner_plugin_active_idx ON plugin_installations(owner_user_id,plugin_id,active,last_checked_at DESC);
CREATE INDEX plugin_installations_owner_device_active_idx ON plugin_installations(owner_user_id,device_id,active);

CREATE TABLE plugin_user_preferences (
 owner_user_id TEXT NOT NULL, plugin_id TEXT NOT NULL REFERENCES plugin_catalog_entries(id) ON DELETE CASCADE,
 enabled BOOLEAN NOT NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL,
 PRIMARY KEY(owner_user_id,plugin_id)
);
CREATE INDEX plugin_user_preferences_owner_enabled_idx ON plugin_user_preferences(owner_user_id,enabled);

CREATE TABLE plugin_component_snapshots (
 plugin_id TEXT NOT NULL REFERENCES plugin_catalog_entries(id) ON DELETE CASCADE,
 release_id TEXT NOT NULL REFERENCES plugin_releases(id) ON DELETE CASCADE,
 component_key TEXT NOT NULL, component_kind TEXT NOT NULL, data JSONB NOT NULL,
 PRIMARY KEY(plugin_id,release_id,component_key)
);
CREATE INDEX plugin_component_snapshots_release_kind_idx ON plugin_component_snapshots(release_id,component_kind);

CREATE TABLE plugin_oauth_connections (
 id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, device_id TEXT NOT NULL,
 plugin_id TEXT NOT NULL REFERENCES plugin_catalog_entries(id) ON DELETE CASCADE,
 release_id TEXT NOT NULL REFERENCES plugin_releases(id) ON DELETE RESTRICT,
 component_key TEXT NOT NULL, provider TEXT NOT NULL, connected BOOLEAN NOT NULL,
 updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL,
 UNIQUE(owner_user_id,device_id,plugin_id,component_key,provider)
);

CREATE TABLE plugin_audit_logs (
 id TEXT PRIMARY KEY, event TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 device_id TEXT NULL, plugin_id TEXT NOT NULL, release_id TEXT NULL,
 component_key TEXT NULL, outcome TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
);
CREATE INDEX plugin_audit_logs_plugin_created_idx ON plugin_audit_logs(plugin_id,created_at DESC);
CREATE INDEX plugin_audit_logs_owner_device_created_idx ON plugin_audit_logs(owner_user_id,device_id,created_at DESC);
