// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) const RUN_SELECT: &str =
    "SELECT run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
     model_config_ref, model_config_revision, capability_policy_revision, input_json, status, \
     iteration, model_attempt, max_iterations, version, claim_token, claim_until_unix_ms, \
     next_attempt_at_unix_ms, pending_tool_batch_json, terminal_outcome_json, \
     checkpoint_json, continuation_input_json, created_at_unix_ms, updated_at_unix_ms \
     FROM local_agent_runs WHERE run_id = ?";

pub(super) const SCHEMA_V1: &[&str] = &[
    "CREATE TABLE local_agent_runs (\
       run_id TEXT PRIMARY KEY NOT NULL,\
       owner_user_id TEXT NOT NULL,\
       owner_entity_type TEXT NOT NULL,\
       owner_entity_id TEXT NOT NULL,\
       profile_key TEXT NOT NULL,\
       model_config_ref TEXT NOT NULL,\
       model_config_revision TEXT NOT NULL,\
       capability_policy_revision TEXT NOT NULL,\
       input_json TEXT NOT NULL,\
       status TEXT NOT NULL CHECK(status IN (\
         'queued','model_ready','model_running','waiting_tool_result',\
         'continuation_ready','waiting_user','retry_scheduled','paused',\
         'needs_review','succeeded','failed','cancelled'\
       )),\
       iteration INTEGER NOT NULL CHECK(iteration >= 0),\
       max_iterations INTEGER NOT NULL CHECK(max_iterations > 0),\
       version INTEGER NOT NULL CHECK(version > 0),\
       claim_token TEXT,\
       claim_until_unix_ms INTEGER,\
       next_attempt_at_unix_ms INTEGER,\
       pending_tool_batch_json TEXT,\
       terminal_outcome_json TEXT,\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL\
     )",
    "CREATE INDEX local_agent_runs_runnable ON local_agent_runs(\
       status, next_attempt_at_unix_ms, created_at_unix_ms, run_id\
     )",
    "CREATE INDEX local_agent_runs_owner ON local_agent_runs(\
       owner_user_id, owner_entity_type, owner_entity_id, updated_at_unix_ms\
     )",
    "CREATE TABLE local_agent_events (\
       cursor INTEGER PRIMARY KEY AUTOINCREMENT,\
       event_id TEXT NOT NULL UNIQUE,\
       run_id TEXT NOT NULL,\
       event_type TEXT NOT NULL,\
       payload_json TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(run_id) REFERENCES local_agent_runs(run_id) ON DELETE CASCADE\
     )",
    "CREATE INDEX local_agent_events_run_cursor ON local_agent_events(run_id, cursor)",
    "CREATE TABLE local_agent_command_receipts (\
       command_id TEXT PRIMARY KEY NOT NULL,\
       request_fingerprint TEXT NOT NULL,\
       response_json TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL\
     )",
];

pub(super) const SCHEMA_V2: &[&str] = &[
    "CREATE TABLE local_agent_tool_invocations (\
       invocation_id TEXT PRIMARY KEY NOT NULL,\
       run_id TEXT NOT NULL,\
       batch_id TEXT NOT NULL,\
       call_id TEXT NOT NULL,\
       tool_name TEXT NOT NULL,\
       arguments_json TEXT NOT NULL,\
       side_effecting INTEGER NOT NULL CHECK(side_effecting IN (0, 1)),\
       status TEXT NOT NULL CHECK(status IN (\
         'pending','running','succeeded','failed','needs_review'\
       )),\
       result_json TEXT,\
       error_text TEXT,\
       version INTEGER NOT NULL CHECK(version > 0),\
       claim_token TEXT,\
       claim_until_unix_ms INTEGER,\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(run_id) REFERENCES local_agent_runs(run_id) ON DELETE CASCADE,\
       UNIQUE(run_id, batch_id, call_id)\
     )",
    "CREATE INDEX local_agent_tool_invocations_claimable ON local_agent_tool_invocations(\
       status, created_at_unix_ms, invocation_id\
     )",
    "CREATE INDEX local_agent_tool_invocations_batch ON local_agent_tool_invocations(\
       run_id, batch_id, status, invocation_id\
     )",
];

pub(super) const SCHEMA_V3: &[&str] = &[
    "ALTER TABLE local_agent_runs ADD COLUMN checkpoint_json TEXT NOT NULL DEFAULT 'null'",
    "ALTER TABLE local_agent_runs ADD COLUMN continuation_input_json TEXT",
];

pub(super) const SCHEMA_V4: &[&str] =
    &["ALTER TABLE local_agent_runs ADD COLUMN model_attempt INTEGER NOT NULL DEFAULT 1 CHECK(model_attempt > 0)"];

pub(super) const SCHEMA_V5: &[&str] = &[
    "CREATE TABLE local_task_graphs (\
       graph_id TEXT PRIMARY KEY NOT NULL,\
       owner_user_id TEXT NOT NULL,\
       source_entity_type TEXT NOT NULL,\
       source_entity_id TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL\
     )",
    "CREATE INDEX local_task_graphs_source ON local_task_graphs(\
       owner_user_id, source_entity_type, source_entity_id, created_at_unix_ms\
     )",
    "CREATE TABLE local_tasks (\
       task_id TEXT PRIMARY KEY NOT NULL,\
       graph_id TEXT NOT NULL,\
       title TEXT NOT NULL,\
       profile_key TEXT NOT NULL,\
       model_config_ref TEXT NOT NULL,\
       model_config_revision TEXT NOT NULL,\
       capability_policy_revision TEXT NOT NULL,\
       input_json TEXT NOT NULL,\
       max_iterations INTEGER NOT NULL CHECK(max_iterations > 0),\
       status TEXT NOT NULL CHECK(status IN (\
         'pending','ready','running','succeeded','failed','cancelled','blocked'\
       )),\
       active_run_id TEXT,\
       version INTEGER NOT NULL CHECK(version > 0),\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(graph_id) REFERENCES local_task_graphs(graph_id) ON DELETE CASCADE,\
       FOREIGN KEY(active_run_id) REFERENCES local_agent_runs(run_id),\
       UNIQUE(graph_id, task_id)\
     )",
    "CREATE INDEX local_tasks_graph_status ON local_tasks(graph_id, status, task_id)",
    "CREATE TABLE local_task_dependencies (\
       graph_id TEXT NOT NULL,\
       task_id TEXT NOT NULL,\
       prerequisite_task_id TEXT NOT NULL,\
       PRIMARY KEY(graph_id, task_id, prerequisite_task_id),\
       FOREIGN KEY(graph_id) REFERENCES local_task_graphs(graph_id) ON DELETE CASCADE,\
       FOREIGN KEY(graph_id, task_id) REFERENCES local_tasks(graph_id, task_id) ON DELETE CASCADE,\
       FOREIGN KEY(graph_id, prerequisite_task_id) REFERENCES local_tasks(graph_id, task_id) ON DELETE CASCADE,\
       CHECK(task_id <> prerequisite_task_id)\
     )",
    "CREATE INDEX local_task_dependencies_prerequisite ON local_task_dependencies(\
       graph_id, prerequisite_task_id, task_id\
     )",
];

pub(super) const SCHEMA_V6: &[&str] = &[
    "CREATE TABLE local_plugin_installations (\
       installation_id TEXT PRIMARY KEY NOT NULL,\
       owner_user_id TEXT NOT NULL,\
       plugin_id TEXT NOT NULL,\
       release_id TEXT NOT NULL,\
       release_digest TEXT NOT NULL,\
       component_id TEXT NOT NULL,\
       component_revision TEXT NOT NULL,\
       server_id TEXT NOT NULL,\
       executable_path TEXT NOT NULL,\
       args_json TEXT NOT NULL,\
       working_directory TEXT,\
       environment_secret_refs_json TEXT NOT NULL,\
       tool_prefix TEXT,\
       allowed_tools_json TEXT,\
       enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),\
       version INTEGER NOT NULL CHECK(version > 0),\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL\
     )",
    "CREATE INDEX local_plugin_installations_owner ON local_plugin_installations(\
       owner_user_id, enabled, updated_at_unix_ms DESC, installation_id\
     )",
    "CREATE UNIQUE INDEX local_plugin_installations_component ON local_plugin_installations(\
       owner_user_id, plugin_id, component_id\
     )",
];

pub(super) const SCHEMA_V7: &[&str] = &[
    "CREATE TABLE local_conversations (\
       conversation_id TEXT PRIMARY KEY NOT NULL,\
       owner_user_id TEXT NOT NULL,\
       title TEXT NOT NULL,\
       version INTEGER NOT NULL CHECK(version > 0),\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL\
     )",
    "CREATE INDEX local_conversations_owner ON local_conversations(\
       owner_user_id, updated_at_unix_ms DESC, conversation_id\
     )",
    "CREATE TABLE local_conversation_turns (\
       turn_id TEXT PRIMARY KEY NOT NULL,\
       conversation_id TEXT NOT NULL,\
       user_message_id TEXT NOT NULL UNIQUE,\
       run_id TEXT NOT NULL UNIQUE,\
       status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(conversation_id) REFERENCES local_conversations(conversation_id) ON DELETE CASCADE,\
       FOREIGN KEY(run_id) REFERENCES local_agent_runs(run_id)\
     )",
    "CREATE UNIQUE INDEX local_conversation_turns_active ON local_conversation_turns(\
       conversation_id\
     ) WHERE status = 'running'",
    "CREATE INDEX local_conversation_turns_conversation ON local_conversation_turns(\
       conversation_id, created_at_unix_ms, turn_id\
     )",
    "CREATE TABLE local_conversation_messages (\
       message_id TEXT PRIMARY KEY NOT NULL,\
       conversation_id TEXT NOT NULL,\
       turn_id TEXT NOT NULL,\
       ordinal INTEGER NOT NULL CHECK(ordinal > 0),\
       role TEXT NOT NULL CHECK(role IN ('user','assistant')),\
       content_json TEXT NOT NULL,\
       metadata_json TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(conversation_id) REFERENCES local_conversations(conversation_id) ON DELETE CASCADE,\
       FOREIGN KEY(turn_id) REFERENCES local_conversation_turns(turn_id) ON DELETE CASCADE,\
       UNIQUE(conversation_id, ordinal)\
     )",
    "CREATE INDEX local_conversation_messages_turn ON local_conversation_messages(\
       turn_id, ordinal\
     )",
];

pub(super) const SCHEMA_V8: &[&str] = &[
    "CREATE TABLE local_conversation_message_attachments (\
       attachment_id TEXT PRIMARY KEY NOT NULL,\
       conversation_id TEXT NOT NULL,\
       turn_id TEXT NOT NULL,\
       message_id TEXT NOT NULL,\
       ordinal INTEGER NOT NULL CHECK(ordinal > 0),\
       display_name TEXT NOT NULL,\
       media_type TEXT NOT NULL,\
       byte_size INTEGER NOT NULL CHECK(byte_size >= 0),\
       sha256 TEXT NOT NULL CHECK(length(sha256) = 64),\
       authorized_local_ref TEXT NOT NULL,\
       metadata_json TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       FOREIGN KEY(conversation_id) REFERENCES local_conversations(conversation_id) ON DELETE CASCADE,\
       FOREIGN KEY(turn_id) REFERENCES local_conversation_turns(turn_id) ON DELETE CASCADE,\
       FOREIGN KEY(message_id) REFERENCES local_conversation_messages(message_id) ON DELETE CASCADE,\
       UNIQUE(message_id, ordinal)\
     )",
    "CREATE INDEX local_conversation_attachments_message ON local_conversation_message_attachments(\
       message_id, ordinal\
     )",
];

pub(super) const SCHEMA_V9: &[&str] = &[
    "CREATE TABLE local_task_graph_writebacks (\
       graph_id TEXT NOT NULL,\
       generation INTEGER NOT NULL CHECK(generation > 0),\
       terminal_signature TEXT NOT NULL,\
       message_id TEXT NOT NULL UNIQUE,\
       terminal_status TEXT NOT NULL CHECK(terminal_status IN ('succeeded','failed','cancelled')),\
       created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(graph_id, generation),\
       FOREIGN KEY(graph_id) REFERENCES local_task_graphs(graph_id) ON DELETE CASCADE,\
       FOREIGN KEY(message_id) REFERENCES local_conversation_messages(message_id) ON DELETE CASCADE,\
       UNIQUE(graph_id, terminal_signature)\
     )",
];

pub(super) const SCHEMA_V10: &[&str] = &[
    "CREATE TABLE local_conversation_guidance (\
       message_id TEXT PRIMARY KEY NOT NULL,\
       conversation_id TEXT NOT NULL,\
       turn_id TEXT NOT NULL,\
       run_id TEXT NOT NULL,\
       payload_json TEXT NOT NULL,\
       delivered_run_version INTEGER,\
       created_at_unix_ms INTEGER NOT NULL,\
       delivered_at_unix_ms INTEGER,\
       FOREIGN KEY(message_id) REFERENCES local_conversation_messages(message_id) ON DELETE CASCADE,\
       FOREIGN KEY(conversation_id) REFERENCES local_conversations(conversation_id) ON DELETE CASCADE,\
       FOREIGN KEY(turn_id) REFERENCES local_conversation_turns(turn_id) ON DELETE CASCADE,\
       FOREIGN KEY(run_id) REFERENCES local_agent_runs(run_id) ON DELETE CASCADE\
     )",
    "CREATE INDEX local_conversation_guidance_pending ON local_conversation_guidance(\
       run_id, delivered_run_version, created_at_unix_ms, message_id\
     )",
];

pub(super) const SCHEMA_V11: &[&str] = &["CREATE TABLE local_capability_policy_snapshots (\
       profile_key TEXT NOT NULL,\
       capability_policy_revision TEXT NOT NULL,\
       instructions TEXT,\
       prefixed_input_items_json TEXT NOT NULL,\
       tools_json TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(profile_key, capability_policy_revision)\
     )"];

pub(super) const SCHEMA_V12: &[&str] = &["CREATE TABLE local_model_config_snapshots (\
       model_config_ref TEXT NOT NULL,\
       model_config_revision TEXT NOT NULL,\
       credential_ref TEXT NOT NULL,\
       base_url TEXT NOT NULL,\
       model TEXT NOT NULL,\
       provider TEXT NOT NULL,\
       supports_responses INTEGER NOT NULL CHECK(supports_responses IN (0, 1)),\
       supports_images INTEGER CHECK(supports_images IN (0, 1)),\
       instructions TEXT,\
       temperature REAL,\
       max_output_tokens INTEGER,\
       thinking_level TEXT,\
       include_prompt_cache_retention INTEGER NOT NULL \
         CHECK(include_prompt_cache_retention IN (0, 1)),\
       request_body_limit_bytes INTEGER,\
       max_transient_retries INTEGER,\
       output_format_json TEXT,\
       created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(model_config_ref, model_config_revision)\
     )"];

pub(super) const SCHEMA_V13: &[&str] = &[
    "CREATE TABLE local_memory_outbox (\
       record_id TEXT NOT NULL,\
       tenant_id TEXT NOT NULL,\
       source_id TEXT NOT NULL,\
       thread_id TEXT NOT NULL,\
       payload_json TEXT NOT NULL,\
       status TEXT NOT NULL CHECK(status IN ('pending','syncing','retry_scheduled','synced')),\
       attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count >= 0),\
       version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),\
       claim_token TEXT,\
       claim_until_unix_ms INTEGER,\
       next_attempt_at_unix_ms INTEGER,\
       last_error TEXT,\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(source_id, record_id)\
     )",
    "CREATE INDEX local_memory_outbox_runnable ON local_memory_outbox(\
       status, next_attempt_at_unix_ms, created_at_unix_ms, source_id, record_id\
     )",
    "CREATE INDEX local_memory_outbox_thread ON local_memory_outbox(\
       tenant_id, source_id, thread_id, created_at_unix_ms, record_id\
     )",
];

pub(super) const SCHEMA_V14: &[&str] = &[
    "CREATE TABLE local_memory_context_cache (\
       cache_key TEXT PRIMARY KEY NOT NULL,\
       tenant_id TEXT NOT NULL,\
       source_id TEXT NOT NULL,\
       thread_id TEXT NOT NULL,\
       response_json TEXT NOT NULL,\
       refreshed_at_unix_ms INTEGER NOT NULL\
     )",
    "CREATE INDEX local_memory_context_cache_thread ON local_memory_context_cache(\
       tenant_id, source_id, thread_id, refreshed_at_unix_ms DESC\
     )",
];

pub(super) const SCHEMA_V15: &[&str] = &["CREATE INDEX local_agent_runs_owner_updated ON \
     local_agent_runs(owner_user_id, updated_at_unix_ms DESC, run_id DESC)"];

pub(super) const SCHEMA_V16: &[&str] = &[
    "ALTER TABLE local_agent_tool_invocations ADD COLUMN \
     requires_approval INTEGER NOT NULL DEFAULT 0 CHECK(requires_approval IN (0, 1))",
    "ALTER TABLE local_agent_tool_invocations ADD COLUMN \
     approval_status TEXT NOT NULL DEFAULT 'not_required' CHECK(approval_status IN (\
       'not_required','pending','approved','rejected'\
     ))",
    "ALTER TABLE local_agent_tool_invocations ADD COLUMN approval_decided_by TEXT",
    "ALTER TABLE local_agent_tool_invocations ADD COLUMN approval_reason TEXT",
    "ALTER TABLE local_agent_tool_invocations ADD COLUMN approval_decided_at_unix_ms INTEGER",
    "CREATE INDEX local_agent_tool_invocations_approval ON local_agent_tool_invocations(\
       approval_status, status, created_at_unix_ms, invocation_id\
     )",
];

pub(super) const SCHEMA_V17: &[&str] =
    &["CREATE INDEX local_plugin_installations_owner_updated ON \
     local_plugin_installations(owner_user_id, updated_at_unix_ms DESC, installation_id DESC)"];

pub(super) const SCHEMA_V18: &[&str] = &[
    "CREATE INDEX local_memory_outbox_tenant_runnable ON local_memory_outbox(\
       tenant_id, status, next_attempt_at_unix_ms, created_at_unix_ms, source_id, record_id\
     )",
];

/// Control-plane snapshots are authenticated-account state. Versions 11 and
/// 12 predated that boundary, so v19 intentionally discards only those two
/// caches instead of assigning ownerless rows to an account.
pub(super) const SCHEMA_V19: &[&str] = &[
    "DROP TABLE local_capability_policy_snapshots",
    "CREATE TABLE local_capability_policy_snapshots (\
       owner_user_id TEXT NOT NULL,\
       profile_key TEXT NOT NULL,\
       capability_policy_revision TEXT NOT NULL,\
       instructions TEXT,\
       prefixed_input_items_json TEXT NOT NULL,\
       tools_json TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, profile_key, capability_policy_revision)\
     )",
    "DROP TABLE local_model_config_snapshots",
    "CREATE TABLE local_model_config_snapshots (\
       owner_user_id TEXT NOT NULL,\
       model_config_ref TEXT NOT NULL,\
       model_config_revision TEXT NOT NULL,\
       credential_ref TEXT NOT NULL,\
       base_url TEXT NOT NULL,\
       model TEXT NOT NULL,\
       provider TEXT NOT NULL,\
       supports_responses INTEGER NOT NULL CHECK(supports_responses IN (0, 1)),\
       supports_images INTEGER CHECK(supports_images IN (0, 1)),\
       instructions TEXT,\
       temperature REAL,\
       max_output_tokens INTEGER,\
       thinking_level TEXT,\
       include_prompt_cache_retention INTEGER NOT NULL \
         CHECK(include_prompt_cache_retention IN (0, 1)),\
       request_body_limit_bytes INTEGER,\
       max_transient_retries INTEGER,\
       output_format_json TEXT,\
       created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, model_config_ref, model_config_revision)\
     )",
    "DELETE FROM local_agent_command_receipts \
       WHERE command_id LIKE 'internal-control-plane-%'",
];

pub(super) const SCHEMA_V20: &[&str] = &[
    "CREATE INDEX local_agent_runs_owner_runnable ON local_agent_runs(\
       owner_user_id, status, next_attempt_at_unix_ms, created_at_unix_ms, run_id\
     )",
    "CREATE INDEX local_agent_tool_invocations_expired ON local_agent_tool_invocations(\
       status, claim_until_unix_ms, run_id, invocation_id\
     )",
];

pub(super) const SCHEMA_V21: &[&str] = &[
    "CREATE TABLE local_conversation_runtime_settings (\
       owner_user_id TEXT NOT NULL,\
       conversation_id TEXT NOT NULL,\
       selected_model_config_ref TEXT NOT NULL,\
       selected_model_config_revision TEXT NOT NULL,\
       selected_thinking_level TEXT,\
       remote_connection_id TEXT,\
       reasoning_enabled INTEGER NOT NULL CHECK(reasoning_enabled IN (0, 1)),\
       version INTEGER NOT NULL CHECK(version > 0),\
       updated_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, conversation_id),\
       FOREIGN KEY(\
         owner_user_id, selected_model_config_ref, selected_model_config_revision\
       ) REFERENCES local_model_config_snapshots(\
         owner_user_id, model_config_ref, model_config_revision\
       )\
     )",
    "CREATE INDEX local_conversation_runtime_settings_model ON \
     local_conversation_runtime_settings(\
       owner_user_id, selected_model_config_ref, selected_model_config_revision\
     )",
];

pub(super) const SCHEMA_V22: &[&str] = &[
    "ALTER TABLE local_conversations ADD COLUMN resource_kind TEXT \
       CHECK(resource_kind IN ('contact','project'))",
    "ALTER TABLE local_conversations ADD COLUMN resource_id TEXT",
    "CREATE UNIQUE INDEX local_conversations_owner_resource ON local_conversations(\
       owner_user_id, resource_kind, resource_id\
     ) WHERE resource_kind IS NOT NULL AND resource_id IS NOT NULL",
];

pub(super) const SCHEMA_V23: &[&str] = &[
    "CREATE TABLE local_notepad_folders (\
       owner_user_id TEXT NOT NULL,\
       path TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, path)\
     )",
    "CREATE TABLE local_notepad_notes (\
       owner_user_id TEXT NOT NULL,\
       note_id TEXT NOT NULL,\
       title TEXT NOT NULL,\
       folder TEXT NOT NULL,\
       content TEXT NOT NULL,\
       tags_json TEXT NOT NULL,\
       version INTEGER NOT NULL CHECK(version > 0),\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, note_id)\
     )",
    "CREATE INDEX local_notepad_notes_owner_updated ON local_notepad_notes(\
       owner_user_id, updated_at_unix_ms DESC, note_id\
     )",
    "CREATE INDEX local_notepad_notes_owner_folder ON local_notepad_notes(\
       owner_user_id, folder, updated_at_unix_ms DESC\
     )",
    "CREATE TABLE local_notepad_images (\
       owner_user_id TEXT NOT NULL,\
       image_id TEXT NOT NULL,\
       note_id TEXT NOT NULL,\
       name TEXT NOT NULL,\
       mime_type TEXT NOT NULL,\
       size INTEGER NOT NULL CHECK(size > 0),\
       sha256 TEXT NOT NULL,\
       data BLOB NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, image_id),\
       FOREIGN KEY(owner_user_id, note_id) REFERENCES local_notepad_notes(\
         owner_user_id, note_id\
       ) ON DELETE CASCADE\
     )",
    "CREATE INDEX local_notepad_images_note ON local_notepad_images(\
       owner_user_id, note_id, created_at_unix_ms\
     )",
];

pub(super) const SCHEMA_V24: &[&str] = &[
    "CREATE TABLE local_remote_connections (\
       owner_user_id TEXT NOT NULL,\
       connection_id TEXT NOT NULL,\
       name TEXT NOT NULL,\
       host TEXT NOT NULL,\
       port INTEGER NOT NULL CHECK(port BETWEEN 1 AND 65535),\
       username TEXT NOT NULL,\
       authentication_type TEXT NOT NULL CHECK(authentication_type IN (\
         'private_key','private_key_cert','password'\
       )),\
       default_remote_path TEXT,\
       host_key_policy TEXT NOT NULL CHECK(host_key_policy IN ('strict','accept_new')),\
       local_connector_device_id TEXT NOT NULL,\
       local_connector_workspace_id TEXT NOT NULL,\
       jump_enabled INTEGER NOT NULL CHECK(jump_enabled IN (0, 1)),\
       jump_connection_id TEXT,\
       jump_host TEXT,\
       jump_port INTEGER CHECK(jump_port BETWEEN 1 AND 65535),\
       jump_username TEXT,\
       last_active_at_unix_ms INTEGER,\
       version INTEGER NOT NULL CHECK(version > 0),\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, connection_id),\
       CHECK(jump_connection_id IS NULL OR jump_connection_id <> connection_id)\
     )",
    "CREATE INDEX local_remote_connections_owner_updated ON local_remote_connections(\
       owner_user_id, updated_at_unix_ms DESC, connection_id\
     )",
];

pub(super) const SCHEMA_V25: &[&str] = &[
    "CREATE TABLE local_agent_artifacts (\
       owner_user_id TEXT NOT NULL,\
       artifact_id TEXT NOT NULL,\
       idempotency_key TEXT NOT NULL,\
       name TEXT NOT NULL,\
       mime_type TEXT NOT NULL,\
       size INTEGER NOT NULL CHECK(size > 0 AND size <= 2097152),\
       sha256 TEXT NOT NULL CHECK(length(sha256) = 64),\
       relative_path TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, artifact_id),\
       UNIQUE(owner_user_id, idempotency_key)\
     )",
    "CREATE INDEX local_agent_artifacts_owner_updated ON local_agent_artifacts(\
       owner_user_id, updated_at_unix_ms DESC, artifact_id DESC\
     )",
];

pub(super) const SCHEMA_V26: &[&str] = &[
    "CREATE TABLE local_requirement_surveys (\
       owner_user_id TEXT NOT NULL,\
       survey_id TEXT NOT NULL,\
       project_resource_id TEXT NOT NULL,\
       source_conversation_id TEXT NOT NULL,\
       source_run_id TEXT NOT NULL,\
       source_task_id TEXT,\
       title TEXT NOT NULL,\
       description TEXT,\
       questions_json TEXT NOT NULL,\
       answers_json TEXT,\
       status TEXT NOT NULL CHECK(status IN ('open','resolved')),\
       version INTEGER NOT NULL CHECK(version > 0),\
       created_at_unix_ms INTEGER NOT NULL,\
       updated_at_unix_ms INTEGER NOT NULL,\
       resolved_at_unix_ms INTEGER,\
       PRIMARY KEY(owner_user_id, survey_id),\
       FOREIGN KEY(source_run_id) REFERENCES local_agent_runs(run_id) ON DELETE CASCADE\
     )",
    "CREATE INDEX local_requirement_surveys_project_status ON local_requirement_surveys(\
       owner_user_id, project_resource_id, status, updated_at_unix_ms DESC, survey_id DESC\
     )",
    "CREATE INDEX local_requirement_surveys_run ON local_requirement_surveys(\
       owner_user_id, source_run_id, status\
     )",
];

pub(super) const SCHEMA_V27: &[&str] = &[
    "ALTER TABLE local_tasks ADD COLUMN cancel_reason TEXT",
    "ALTER TABLE local_tasks ADD COLUMN replacement_task_ids_json TEXT NOT NULL DEFAULT '[]'",
    "ALTER TABLE local_tasks ADD COLUMN cancelled_because_task_id TEXT",
    "ALTER TABLE local_tasks ADD COLUMN cascade_root_task_id TEXT",
];

pub(super) const SCHEMA_V28: &[&str] = &[
    "CREATE TABLE local_task_external_dependencies (\
       task_id TEXT NOT NULL,\
       prerequisite_task_id TEXT NOT NULL,\
       PRIMARY KEY(task_id, prerequisite_task_id),\
       FOREIGN KEY(task_id) REFERENCES local_tasks(task_id) ON DELETE CASCADE,\
       FOREIGN KEY(prerequisite_task_id) REFERENCES local_tasks(task_id) ON DELETE CASCADE,\
       CHECK(task_id <> prerequisite_task_id)\
     )",
    "CREATE INDEX local_task_external_dependencies_prerequisite ON \
       local_task_external_dependencies(prerequisite_task_id, task_id)",
];

/// Failed conversation Runs used to update only the Turn status. Backfill the
/// missing assistant message so historical failures are visible in Main Chat.
pub(super) const SCHEMA_V29: &[&str] = &[
    r#"WITH missing_failed_messages AS (
         SELECT t.turn_id, t.conversation_id, t.run_id,
                r.terminal_outcome_json, r.updated_at_unix_ms,
                COALESCE((
                  SELECT MAX(stored_message.ordinal)
                  FROM local_conversation_messages stored_message
                  WHERE stored_message.conversation_id = t.conversation_id
                ), 0) AS current_max_ordinal,
                ROW_NUMBER() OVER (
                  PARTITION BY t.conversation_id
                  ORDER BY r.updated_at_unix_ms, t.turn_id
                ) AS missing_ordinal
         FROM local_conversation_turns t
         JOIN local_agent_runs r ON r.run_id = t.run_id
         WHERE t.status = 'failed'
           AND r.status = 'failed'
           AND r.owner_entity_type = 'conversation_turn'
           AND NOT EXISTS (
             SELECT 1 FROM local_conversation_messages assistant
             WHERE assistant.turn_id = t.turn_id AND assistant.role = 'assistant'
           )
       )
       INSERT INTO local_conversation_messages(
         message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json,
         created_at_unix_ms
       )
       SELECT 'assistant:' || run_id, conversation_id, turn_id,
              current_max_ordinal + missing_ordinal, 'assistant',
              COALESCE(terminal_outcome_json,
                '{"error":"Local model execution failed before producing a response."}'),
              json_object('run_id', run_id, 'terminal_status', 'failed'),
              updated_at_unix_ms
       FROM missing_failed_messages"#,
    r#"UPDATE local_conversations
       SET version = version + 1,
           updated_at_unix_ms = MAX(updated_at_unix_ms, COALESCE((
             SELECT MAX(r.updated_at_unix_ms)
             FROM local_conversation_turns t
             JOIN local_agent_runs r ON r.run_id = t.run_id
             WHERE t.conversation_id = local_conversations.conversation_id
               AND t.status = 'failed'
               AND r.status = 'failed'
           ), updated_at_unix_ms))
       WHERE EXISTS (
         SELECT 1
         FROM local_conversation_turns t
         JOIN local_conversation_messages m ON m.turn_id = t.turn_id
         WHERE t.conversation_id = local_conversations.conversation_id
           AND t.status = 'failed'
           AND m.role = 'assistant'
           AND json_extract(m.metadata_json, '$.terminal_status') = 'failed'
       )"#,
];

pub(super) const SCHEMA_V30: &[&str] = &[
    "CREATE TABLE local_task_followup_reviews (\
       task_id TEXT PRIMARY KEY NOT NULL REFERENCES local_tasks(task_id) ON DELETE CASCADE,\
       conversation_id TEXT NOT NULL REFERENCES local_conversations(conversation_id) ON DELETE CASCADE,\
       main_chat_run_id TEXT NOT NULL REFERENCES local_agent_runs(run_id) ON DELETE CASCADE,\
       review_message_id TEXT NOT NULL,\
       state TEXT NOT NULL CHECK(state IN ('pending','held','released','cancelled')),\
       reason TEXT, updated_at_unix_ms INTEGER NOT NULL\
     )",
    "CREATE INDEX local_task_followup_reviews_run_state ON local_task_followup_reviews(main_chat_run_id, state)",
];

/// Persist which runtime owns each Tool invocation. Existing platform tools remain platform-owned,
/// while the built-in control-plane tools are backfilled to the shared Rust worker. Unified
/// `agent_skill_*` calls belong to macOS only for Task Execution Runs.
pub(super) const SCHEMA_V31: &[&str] = &[
    "ALTER TABLE local_agent_tool_invocations ADD COLUMN execution_route TEXT NOT NULL \
       DEFAULT 'platform' CHECK(execution_route IN ('shared','platform'))",
    "UPDATE local_agent_tool_invocations SET execution_route = 'shared' WHERE \
       tool_name IN (\
         'list_tasks','get_task','create_task','create_tasks_with_prerequisites',\
         'cancel_task','wait_for_task_completion','get_task_dependency_graph',\
         'resolve_task_followup','ask_user_prompt_key_values','ask_user_prompt_choices',\
         'ask_user_prompt_mixed_form','notepad_list_folders','notepad_list_notes',\
         'notepad_read_note','notepad_create_note','notepad_update_note',\
         'requirement_survey_create','task_run_process_record_process',\
         'task_run_process_report_outcome'\
       ) OR (tool_name IN (\
         'agent_skill_activate','agent_skill_read_resource'\
       ) AND run_id IN (\
         SELECT run_id FROM local_agent_runs WHERE profile_key <> 'task_execution'\
       ))",
    "CREATE INDEX local_agent_tool_invocations_route_claimable ON \
       local_agent_tool_invocations(execution_route, status, created_at_unix_ms, invocation_id)",
];

pub(super) const SCHEMA_V32: &[&str] = &[
    "CREATE TABLE local_requirement_plans (\
       owner_user_id TEXT NOT NULL, plan_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), \
       project_resource_id TEXT NOT NULL, source_conversation_id TEXT NOT NULL, \
       source_survey_id TEXT NOT NULL, source_run_id TEXT NOT NULL, title TEXT NOT NULL, \
       markdown TEXT NOT NULL, created_at_unix_ms INTEGER NOT NULL, confirmed_at_unix_ms INTEGER, \
       PRIMARY KEY(owner_user_id, plan_id, revision), \
       FOREIGN KEY(owner_user_id, source_survey_id) REFERENCES local_requirement_surveys(owner_user_id, survey_id) ON DELETE CASCADE, \
       FOREIGN KEY(source_run_id) REFERENCES local_agent_runs(run_id) ON DELETE CASCADE)",
    "CREATE INDEX local_requirement_plans_project ON local_requirement_plans(\
       owner_user_id, project_resource_id, created_at_unix_ms DESC, plan_id, revision DESC)",
];

/// Model replies remain private to a claim until its Run transition commits.
pub(super) const SCHEMA_V34: &[&str] = &[
    "CREATE TABLE local_agent_step_memory (\
       run_id TEXT NOT NULL REFERENCES local_agent_runs(run_id) ON DELETE CASCADE,\
       claim_token TEXT NOT NULL, run_version INTEGER NOT NULL,\
       record_id TEXT NOT NULL, tenant_id TEXT NOT NULL, source_id TEXT NOT NULL,\
       thread_id TEXT NOT NULL, payload_json TEXT NOT NULL, created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(run_id, claim_token, source_id, record_id)\
     )",
    "CREATE TRIGGER local_agent_step_memory_discard AFTER UPDATE OF version, claim_token, status \
       ON local_agent_runs WHEN NEW.version <> OLD.version \
         OR NEW.claim_token IS NOT OLD.claim_token OR NEW.status <> 'model_running' BEGIN \
         DELETE FROM local_agent_step_memory WHERE run_id = NEW.run_id; \
       END",
];

/// The response cache remains transient, but v35 retains write identities separately.
/// Preserve existing receipts on future upgrades through v33: deleting them here
/// would destroy the identities before v35 can backfill them. Already deleted
/// receipts in databases previously migrated by older binaries cannot be recovered.
/// Synced Memory rows are delivery receipts, not the authoritative Memory store.
pub(super) const SCHEMA_V33: &[&str] = &[
    "CREATE INDEX local_agent_command_receipts_retention ON \
       local_agent_command_receipts(created_at_unix_ms DESC, command_id DESC)",
    "CREATE TRIGGER local_agent_command_receipts_prune AFTER INSERT ON \
       local_agent_command_receipts BEGIN \
         DELETE FROM local_agent_command_receipts \
           WHERE created_at_unix_ms < (\
             SELECT MAX(created_at_unix_ms) - 86400000 \
             FROM local_agent_command_receipts\
           ); \
         DELETE FROM local_agent_command_receipts WHERE command_id IN (\
           SELECT command_id FROM local_agent_command_receipts \
             ORDER BY created_at_unix_ms DESC, command_id DESC LIMIT -1 OFFSET 5000\
         ); \
       END",
    "DELETE FROM local_memory_outbox WHERE status = 'synced'",
    "CREATE INDEX local_memory_outbox_synced_retention ON local_memory_outbox(\
       status, updated_at_unix_ms DESC, source_id, record_id\
     )",
    "CREATE TRIGGER local_memory_outbox_prune AFTER UPDATE OF status ON local_memory_outbox \
       WHEN NEW.status = 'synced' BEGIN \
         DELETE FROM local_memory_outbox WHERE status = 'synced' \
           AND (source_id, record_id) IN (\
             SELECT source_id, record_id FROM local_memory_outbox WHERE status = 'synced' \
               ORDER BY updated_at_unix_ms DESC, source_id, record_id LIMIT -1 OFFSET 1000\
           ); \
       END",
];

/// Durable write identity is independent of the evictable historical response.
/// A tombstone means the local transaction completed, not that an external tool
/// or provider executed exactly once. Deliberately no entity foreign key or TTL.
pub(super) const SCHEMA_V35: &[&str] = &[
    "CREATE TABLE local_agent_command_tombstones (\
       command_id TEXT PRIMARY KEY NOT NULL,\
       request_fingerprint TEXT NOT NULL,\
       completed_at_unix_ms INTEGER NOT NULL\
     )",
    // BEFORE INSERT is essential: the existing AFTER INSERT pruning trigger
    // can immediately evict an old-timestamp response, but not its identity.
    "CREATE TRIGGER local_agent_command_receipts_dedupe BEFORE INSERT ON \
       local_agent_command_receipts BEGIN \
         INSERT INTO local_agent_command_tombstones(\
           command_id, request_fingerprint, completed_at_unix_ms\
         ) VALUES(NEW.command_id, NEW.request_fingerprint, NEW.created_at_unix_ms); \
       END",
];
