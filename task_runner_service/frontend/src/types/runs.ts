export type TaskRunStatus =
  | 'queued'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'blocked';

export interface TaskRunRecord {
  id: string;
  task_id: string;
  model_config_id: string;
  memory_thread_id: string;
  status: TaskRunStatus;
  started_at?: string | null;
  finished_at?: string | null;
  input_snapshot: unknown;
  context_snapshot?: unknown;
  result_summary?: string | null;
  error_message?: string | null;
  usage?: unknown;
  report?: unknown;
  cancel_requested: boolean;
  summary_job_run_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface TaskRunEventRecord {
  id: string;
  run_id: string;
  event_type: string;
  message?: string | null;
  payload?: unknown;
  created_at: string;
}

export interface RunSummaryRecord {
  id: string;
  task_id: string;
  status: TaskRunStatus;
  model_config_id: string;
  updated_at: string;
}

export interface StartTaskRunPayload {
  model_config_id?: string;
  prompt_override?: string;
}

export interface RunListFilters {
  task_id?: string;
  status?: TaskRunStatus;
  model_config_id?: string;
  keyword?: string;
  limit?: number;
  offset?: number;
}
