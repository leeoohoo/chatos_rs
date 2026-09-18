-- SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
-- Required Notice: Copyright (c) 2025 AI Chat Team

CREATE TABLE agents (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,enabled BOOLEAN NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX agents_user_updated_idx ON agents(user_id,updated_at DESC);

CREATE TABLE applications (id TEXT PRIMARY KEY,user_id TEXT NULL,name TEXT NOT NULL,enabled BOOLEAN NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX applications_user_created_idx ON applications(user_id,created_at DESC);

CREATE TABLE chatos_contacts (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,agent_id TEXT NOT NULL,status TEXT NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL,UNIQUE(user_id,agent_id));
CREATE INDEX chatos_contacts_user_status_idx ON chatos_contacts(user_id,status,updated_at DESC);

CREATE TABLE chatos_memory_projects (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,project_id TEXT NOT NULL,status TEXT NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL,UNIQUE(user_id,project_id));
CREATE INDEX chatos_memory_projects_user_updated_idx ON chatos_memory_projects(user_id,updated_at DESC);

CREATE TABLE chatos_project_agent_links (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,project_id TEXT NOT NULL,agent_id TEXT NOT NULL,contact_id TEXT NULL,status TEXT NOT NULL,last_bound_at TIMESTAMPTZ NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL,UNIQUE(user_id,project_id),UNIQUE(user_id,project_id,agent_id));
CREATE INDEX chatos_project_agent_links_contact_idx ON chatos_project_agent_links(user_id,contact_id,status,last_bound_at DESC);
CREATE INDEX chatos_project_agent_links_project_idx ON chatos_project_agent_links(user_id,project_id,status,last_bound_at DESC);

CREATE TABLE memory_skills (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,plugin_source TEXT NOT NULL,name TEXT NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX memory_skills_listing_idx ON memory_skills(user_id,plugin_source,updated_at DESC);
CREATE TABLE memory_skill_plugins (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,source TEXT NOT NULL,installed BOOLEAN NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL,UNIQUE(user_id,source));
CREATE INDEX memory_skill_plugins_listing_idx ON memory_skill_plugins(user_id,updated_at DESC);

CREATE TABLE pet_activity_inbox (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,activity_key TEXT NOT NULL,activity_version TEXT NOT NULL,inbox_status TEXT NOT NULL,occurred_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,expires_at TIMESTAMPTZ NULL,data JSONB NOT NULL,UNIQUE(user_id,activity_key,activity_version));
CREATE INDEX pet_activity_inbox_status_idx ON pet_activity_inbox(user_id,inbox_status,occurred_at DESC);
CREATE INDEX pet_activity_inbox_updated_idx ON pet_activity_inbox(user_id,updated_at DESC);

CREATE TABLE remote_connections (id TEXT PRIMARY KEY,user_id TEXT NULL,host TEXT NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,last_active_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX remote_connections_user_idx ON remote_connections(user_id);
CREATE INDEX remote_connections_host_idx ON remote_connections(host);

CREATE TABLE session_runtime_settings (session_id TEXT PRIMARY KEY,user_id TEXT NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX session_runtime_settings_user_idx ON session_runtime_settings(user_id,updated_at DESC);

CREATE TABLE system_contexts (id TEXT PRIMARY KEY,user_id TEXT NOT NULL,is_active BOOLEAN NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE UNIQUE INDEX system_contexts_one_active_per_user ON system_contexts(user_id) WHERE is_active;
CREATE TABLE system_context_applications (id TEXT PRIMARY KEY,system_context_id TEXT NOT NULL REFERENCES system_contexts(id) ON DELETE CASCADE,application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,created_at TIMESTAMPTZ NOT NULL,UNIQUE(system_context_id,application_id));

CREATE TABLE terminals (id TEXT PRIMARY KEY,user_id TEXT NULL,project_id TEXT NULL,kind TEXT NOT NULL,status TEXT NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,last_active_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX terminals_user_kind_idx ON terminals(user_id,kind,created_at DESC);
CREATE INDEX terminals_project_idx ON terminals(project_id);
CREATE INDEX terminals_status_idx ON terminals(status);
CREATE TABLE terminal_logs (id TEXT PRIMARY KEY,terminal_id TEXT NOT NULL REFERENCES terminals(id) ON DELETE CASCADE,created_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX terminal_logs_terminal_created_idx ON terminal_logs(terminal_id,created_at);

CREATE TABLE user_settings (user_id TEXT PRIMARY KEY,updated_at TIMESTAMPTZ NOT NULL,settings JSONB NOT NULL);

CREATE TABLE task_manager_tasks (id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL,conversation_turn_id TEXT NOT NULL,status TEXT NOT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX task_manager_tasks_conversation_turn_idx ON task_manager_tasks(conversation_id,conversation_turn_id);
CREATE INDEX task_manager_tasks_conversation_created_idx ON task_manager_tasks(conversation_id,created_at DESC);
CREATE INDEX task_manager_tasks_turn_created_idx ON task_manager_tasks(conversation_turn_id,created_at DESC);

CREATE TABLE ask_user_prompt_requests (id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL,conversation_turn_id TEXT NOT NULL,status TEXT NOT NULL,source TEXT NOT NULL,external_prompt_id TEXT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,expires_at TIMESTAMPTZ NULL,data JSONB NOT NULL);
CREATE INDEX ask_user_prompt_conversation_status_idx ON ask_user_prompt_requests(conversation_id,status,updated_at DESC);
CREATE INDEX ask_user_prompt_turn_created_idx ON ask_user_prompt_requests(conversation_turn_id,created_at DESC);
CREATE UNIQUE INDEX ask_user_prompt_external_idx ON ask_user_prompt_requests(source,external_prompt_id) WHERE external_prompt_id IS NOT NULL;

CREATE TABLE cloud_agent_lanes (ordering_lane_key TEXT PRIMARY KEY,next_lane_seq BIGINT NOT NULL,active_lane_seq BIGINT NOT NULL,version BIGINT NOT NULL,updated_at TIMESTAMPTZ NOT NULL);
CREATE TABLE cloud_agent_runs (agent_run_id TEXT PRIMARY KEY,ordering_lane_key TEXT NOT NULL REFERENCES cloud_agent_lanes(ordering_lane_key),lane_seq BIGINT NOT NULL,generation BIGINT NOT NULL,step_seq BIGINT NOT NULL,status TEXT NOT NULL,phase TEXT NOT NULL,version BIGINT NOT NULL,claim_token TEXT NULL,claim_until TIMESTAMPTZ NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL,UNIQUE(ordering_lane_key,lane_seq));
CREATE INDEX cloud_agent_runs_claim_idx ON cloud_agent_runs(status,claim_until);
CREATE TABLE cloud_agent_outbox (event_id TEXT PRIMARY KEY,agent_run_id TEXT NOT NULL REFERENCES cloud_agent_runs(agent_run_id) ON DELETE CASCADE,status TEXT NOT NULL,available_at TIMESTAMPTZ NOT NULL,publish_attempts INTEGER NOT NULL DEFAULT 0,last_error TEXT NULL,created_at TIMESTAMPTZ NOT NULL,updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL);
CREATE INDEX cloud_agent_outbox_ready_idx ON cloud_agent_outbox(status,available_at,event_id);
