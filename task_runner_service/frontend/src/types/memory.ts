export interface EngineThread {
  id: string;
  tenant_id: string;
  source_id: string;
  subject_id: string;
  thread_type: string;
  external_thread_id?: string | null;
  title?: string | null;
  labels?: string[] | null;
  metadata?: unknown;
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
  structured_payload?: unknown;
  metadata?: unknown;
  summary_status: string;
  summary_id?: string | null;
  summarized_at?: string | null;
  created_at: string;
}

export interface ComposeContextBlock {
  block_type: string;
  text: string;
}

export interface ComposeContextMeta {
  summary_count: number;
  recent_record_count: number;
}

export interface ComposeContextResponse {
  thread_id: string;
  blocks: ComposeContextBlock[];
  recent_records: EngineRecord[];
  meta: ComposeContextMeta;
}

export interface TaskMemoryContextResponse {
  task_id: string;
  memory_thread_id: string;
  tenant_id: string;
  subject_id: string;
  thread?: EngineThread | null;
  context?: ComposeContextResponse | null;
  total_record_count: number;
}

export interface TaskMemoryRecordsResponse {
  task_id: string;
  memory_thread_id: string;
  tenant_id: string;
  subject_id: string;
  thread?: EngineThread | null;
  total: number;
  limit: number;
  offset: number;
  order: string;
  role?: string | null;
  record_type?: string | null;
  summary_status?: string | null;
  has_more: boolean;
  items: EngineRecord[];
}

export interface TaskMemorySummaryJobResult {
  thread_id: string;
  accepted: boolean;
  running: boolean;
  job_run_id?: string | null;
  generated: boolean;
  summary_id?: string | null;
  source_record_count: number;
}

export interface TaskMemorySummaryResponse {
  task_id: string;
  memory_thread_id: string;
  tenant_id: string;
  requested_at: string;
  result: TaskMemorySummaryJobResult;
}
