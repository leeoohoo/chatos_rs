// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type RemoteServerAuthType = 'password' | 'private_key' | 'private_key_cert';
export type RemoteServerHostKeyPolicy = 'accept_new' | 'strict';
export type RemoteServerTestStatus = 'success' | 'failed';

export interface RemoteServerRecord {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: RemoteServerAuthType | string;
  password?: string | null;
  private_key_path?: string | null;
  certificate_path?: string | null;
  default_remote_path?: string | null;
  host_key_policy: RemoteServerHostKeyPolicy | string;
  enabled: boolean;
  last_tested_at?: string | null;
  last_test_status?: RemoteServerTestStatus | string | null;
  last_test_message?: string | null;
  last_active_at?: string | null;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  task_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateRemoteServerPayload {
  name: string;
  host: string;
  port?: number;
  username: string;
  auth_type: RemoteServerAuthType | string;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: RemoteServerHostKeyPolicy | string;
  enabled?: boolean;
}

export interface UpdateRemoteServerPayload {
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: RemoteServerAuthType | string;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: RemoteServerHostKeyPolicy | string;
  enabled?: boolean;
}

export interface TestRemoteServerPayload {
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: RemoteServerAuthType | string;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: RemoteServerHostKeyPolicy | string;
}

export interface RemoteServerTestResponse {
  ok: boolean;
  server_id?: string | null;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: RemoteServerAuthType | string;
  remote_host?: string | null;
  error?: string | null;
  tested_at: string;
}

export type ExternalMcpTransport = 'stdio' | 'http';

export interface ExternalMcpConfigRecord {
  id: string;
  name: string;
  transport: ExternalMcpTransport | string;
  command?: string | null;
  args: string[];
  url?: string | null;
  headers: Record<string, string>;
  env: Record<string, string>;
  cwd?: string | null;
  enabled: boolean;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateExternalMcpConfigPayload {
  name: string;
  transport: ExternalMcpTransport | string;
  command?: string;
  args?: string[];
  url?: string;
  headers?: Record<string, string>;
  env?: Record<string, string>;
  cwd?: string;
  enabled?: boolean;
}

export interface UpdateExternalMcpConfigPayload {
  name?: string;
  transport?: ExternalMcpTransport | string;
  command?: string;
  args?: string[];
  url?: string;
  headers?: Record<string, string>;
  env?: Record<string, string>;
  cwd?: string;
  enabled?: boolean;
}
