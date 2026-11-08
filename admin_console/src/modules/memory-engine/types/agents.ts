// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface EngineAgentDirectoryItem {
  tenant_id: string;
  source_id: string;
  agent_subject_id: string;
  agent_id: string;
  display_name?: string | null;
  thread_count: number;
  project_count: number;
  memory_count: number;
  updated_at: string;
}

export interface EngineAgentProject {
  tenant_id: string;
  source_id: string;
  project_subject_id: string;
  project_id: string;
  display_name?: string | null;
  thread_count: number;
  memory_count: number;
  updated_at: string;
}

export interface EngineAgentConversation {
  tenant_id: string;
  source_id: string;
  agent_subject_id: string;
  thread_id: string;
  project_subject_id?: string | null;
  project_display_name?: string | null;
  conversation_id?: string | null;
  thread_kind: string;
  record_count: number;
  status: string;
  summary_status: string;
  updated_at: string;
}

export interface AgentDirectoryQuery {
  tenant_id?: string;
  source_id?: string;
  agent_id?: string;
  limit?: number;
  offset?: number;
}
