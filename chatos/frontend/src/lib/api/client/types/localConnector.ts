// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ProjectResponse } from './project';

export interface LocalConnectorDeviceResponse {
  id: string;
  owner_user_id?: string;
  ownerUserId?: string;
  display_name?: string;
  displayName?: string;
  public_key?: string;
  publicKey?: string;
  client_version?: string | null;
  clientVersion?: string | null;
  os?: string | null;
  status?: string;
  last_seen_at?: string | null;
  lastSeenAt?: string | null;
  revoked_at?: string | null;
  revokedAt?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface LocalConnectorWorkspaceResponse {
  id: string;
  owner_user_id?: string;
  ownerUserId?: string;
  device_id?: string;
  deviceId?: string;
  display_name?: string;
  displayName?: string;
  local_path_alias?: string;
  localPathAlias?: string;
  local_path_fingerprint?: string;
  localPathFingerprint?: string;
  capabilities?: string[];
  status?: string;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface TaskRunnerSelectablePluginCommandResponse {
  command_id: string;
  display_name: string;
  description?: string | null;
  argument_hint?: string | null;
  requires_confirmation: boolean;
  target_agent?: string | null;
  allowed_tools?: string[];
}

export interface TaskRunnerSelectablePluginComponentResponse {
  component_key: string;
  kind: 'skill_collection' | 'mcp_server' | 'connected_app' | 'command' | 'agent' | 'hook_set' | 'ui_contribution';
  execution_host: 'local' | 'portable';
  available: boolean;
  status: string;
  reason?: string | null;
  content_sha256?: string | null;
  prepare_provider: 'mcp_management';
  requires_workspace: boolean;
}

export interface TaskRunnerSelectablePluginResponse {
  id: string;
  plugin_key: string;
  display_name: string;
  description: string;
  version: string;
  release_id: string;
  artifact_sha256: string;
  device_id?: string | null;
  execution_type: 'local' | 'portable' | 'hybrid';
  requires_device: boolean;
  component_hosts: Record<string, 'local' | 'portable'>;
  component_keys: string[];
  components: TaskRunnerSelectablePluginComponentResponse[];
  commands: TaskRunnerSelectablePluginCommandResponse[];
}

export interface TaskRunnerAvailablePluginsResponse {
  agent_key: string;
  policy_revision: string;
  selectable_plugins: TaskRunnerSelectablePluginResponse[];
}

export interface LocalConnectorProjectBindingResponse {
  id: string;
  owner_user_id?: string;
  ownerUserId?: string;
  project_id?: string;
  projectId?: string;
  device_id?: string;
  deviceId?: string;
  workspace_id?: string;
  workspaceId?: string;
  mode?: string;
  enabled?: boolean;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface CreateLocalConnectorProjectRequest {
  name?: string;
  device_id: string;
  workspace_id: string;
  relative_path?: string;
  git_url?: string;
  description?: string;
  user_id?: string;
}

export interface LocalConnectorDirectoryEntryResponse {
  name?: string;
  path?: string;
  is_dir?: boolean;
  isDir?: boolean;
  len?: number;
}

export interface LocalConnectorDirectoryListResponse {
  path?: string;
  parent?: string | null;
  entries?: LocalConnectorDirectoryEntryResponse[];
}

export interface CreateLocalConnectorDirectoryRequest {
  device_id: string;
  workspace_id: string;
  path: string;
  user_id?: string;
}

export interface CreateLocalConnectorDirectoryResponse {
  path?: string;
  created?: boolean;
}

export interface LocalConnectorTerminalExecRequest {
  device_id: string;
  workspace_id: string;
  command: string;
  args?: string[];
  cwd?: string;
  timeout_ms?: number;
  user_id?: string;
}

export interface LocalConnectorTerminalExecResponse {
  success?: boolean;
  exit_code?: number | null;
  exitCode?: number | null;
  stdout?: string;
  stderr?: string;
  timed_out?: boolean;
  timedOut?: boolean;
  truncated?: boolean;
  command?: string;
  cwd?: string;
  error?: string;
  [key: string]: unknown;
}

export interface LocalConnectorProjectMetadataResponse {
  device?: LocalConnectorDeviceResponse;
  workspace?: LocalConnectorWorkspaceResponse;
  bindings?: LocalConnectorProjectBindingResponse[];
}

export interface LocalConnectorProjectResponse extends ProjectResponse {
  local_connector?: LocalConnectorProjectMetadataResponse;
  localConnector?: LocalConnectorProjectMetadataResponse;
}
