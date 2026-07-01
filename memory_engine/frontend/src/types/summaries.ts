// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface EngineSummary {
  id: string;
  tenant_id: string;
  source_id: string;
  thread_id: string;
  subject_id: string;
  summary_type: string;
  level: number;
  source_digest?: string | null;
  summary_text: string;
  source_record_start_id?: string | null;
  source_record_end_id?: string | null;
  source_record_count: number;
  status: string;
  rollup_status: string;
  rollup_summary_id?: string | null;
  rolled_up_at?: string | null;
  subject_memory_summarized: number;
  subject_memory_summarized_at?: string | null;
  metadata?: Record<string, unknown> | null;
  created_at: string;
  updated_at: string;
}

export interface EngineSubjectMemory {
  id: string;
  tenant_id: string;
  source_id: string;
  subject_id: string;
  memory_key: string;
  memory_type: string;
  text: string;
  level: number;
  source_digest?: string | null;
  confidence?: number | null;
  last_seen_at?: string | null;
  metadata?: Record<string, unknown> | null;
  status: string;
  rollup_status: string;
  rollup_memory_key?: string | null;
  rolled_up_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ThreadSummariesQuery {
  tenant_id?: string;
  source_id?: string;
  summary_type?: string;
  status?: string;
  level?: number;
  limit?: number;
  offset?: number;
}

export interface SubjectMemoriesQuery {
  tenant_id: string;
  source_id: string;
  memory_type?: string;
  level?: number;
  limit?: number;
  offset?: number;
}
