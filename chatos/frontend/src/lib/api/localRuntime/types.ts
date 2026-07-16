// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface LocalRuntimeBridgeResponse {
  status: number;
  ok: boolean;
  headers?: Record<string, string | string[] | undefined>;
  body?: string;
}

export interface LocalRuntimeBridgeRequest {
  endpoint: string;
  method?: string;
  headers?: Record<string, string>;
  body?: string | null;
}

export interface LocalRuntimeProjectRecord {
  project_id: string;
  owner_user_id: string;
  device_id: string;
  workspace_id: string;
  project_name: string;
  root_relative_path?: string | null;
  execution_plane: 'local_connector';
  runtime_schema_version: number;
  created_at: string;
  updated_at: string;
}

export interface LocalRuntimeSessionRecord {
  id: string;
  project_id: string;
  owner_user_id: string;
  title: string;
  selected_model_id?: string | null;
  selected_agent_id?: string | null;
  status: string;
  message_count: number;
  created_at: string;
  updated_at: string;
}

export interface LocalRuntimeEventRecord {
  event_seq: number;
  event_id: string;
  project_id?: string | null;
  session_id?: string | null;
  turn_id?: string | null;
  event_name: string;
  stream_type?: string | null;
  payload: unknown;
  created_at: string;
}
