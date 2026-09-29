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
