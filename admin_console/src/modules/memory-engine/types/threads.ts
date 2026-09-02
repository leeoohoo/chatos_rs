// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface EngineThread {
  id: string;
  tenant_id: string;
  source_id: string;
  subject_id: string;
  thread_type: string;
  external_thread_id?: string | null;
  title?: string | null;
  labels?: string[] | null;
  metadata?: Record<string, unknown> | null;
  status: string;
  summary_status: string;
  summary_job_run_id?: string | null;
  summary_locked_at?: string | null;
  summary_lock_expires_at?: string | null;
  pending_record_count: number;
  pending_summary_tokens: number;
  created_at: string;
  updated_at: string;
  archived_at?: string | null;
}

export interface EngineRecord {
  id: string;
  thread_id: string;
  tenant_id: string;
  source_id: string;
  external_record_id?: string | null;
  role: string;
  record_type: string;
  content: string;
  structured_payload?: Record<string, unknown> | null;
  metadata?: Record<string, unknown> | null;
  summary_status: string;
  summary_id?: string | null;
  summarized_at?: string | null;
  created_at: string;
}

export interface ThreadQuery {
  tenant_id?: string;
  source_id?: string;
  subject_id?: string;
  external_thread_id?: string;
  session_id?: string;
  contact_id?: string;
  project_id?: string;
  agent_id?: string;
  mapping_source?: string;
  mapping_version?: string;
  thread_label?: string;
  status?: string;
  limit?: number;
  offset?: number;
}

export interface ThreadRecordsQuery {
  tenant_id?: string;
  source_id?: string;
  role?: string;
  record_type?: string;
  summary_status?: string;
  limit?: number;
  offset?: number;
  order?: string;
}

export interface ThreadRecordsPage {
  items: EngineRecord[];
  total: number;
}
