// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) const RUN_SELECT: &str =
    "SELECT run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
     model_config_ref, model_config_revision, capability_policy_revision, input_json, status, \
     iteration, max_iterations, version, claim_token, claim_until_unix_ms, \
     next_attempt_at_unix_ms, pending_tool_batch_json, terminal_outcome_json, \
     created_at_unix_ms, updated_at_unix_ms FROM local_agent_runs WHERE run_id = ?";

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
