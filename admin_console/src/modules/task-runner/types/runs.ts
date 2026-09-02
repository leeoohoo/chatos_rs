// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type TaskRunStatus =
  | 'queued'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'blocked';

export type TaskRunAttemptStatus =
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'blocked'
  | 'interrupted';

export type ModelPhaseStatus =
  | 'pending'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'blocked';

export type WorkspaceIntegrationStatus =
  | 'not_required'
  | 'pending'
  | 'integrating'
  | 'integrated'
  | 'waived'
  | 'conflict'
  | 'failed';

export interface TaskRunWorkspaceExecution {
  execution_group_id?: string | null;
  execution_branch_ref?: string | null;
  integration_status: WorkspaceIntegrationStatus;
  result_commit?: string | null;
  integrated_commit?: string | null;
  promoted_commit?: string | null;
  waived_at?: string | null;
  waiver_reason?: string | null;
  local_changed_files?: RunWorkspaceChangedFile[];
  local_patch?: string | null;
  local_patch_truncated?: boolean;
  conflict_files?: string[];
  conflict_message?: string | null;
  integration_last_error?: string | null;
  integration_attempt_count?: number;
}

export interface RunWorkspaceChangedFile {
  status: string;
  path: string;
  old_path?: string | null;
}

export interface RunWorkspaceChanges {
  project_id: string;
  run_id: string;
  branch_ref: string;
  base_commit: string;
  result_commit: string;
  files: RunWorkspaceChangedFile[];
  patch: string;
  patch_truncated: boolean;
}

export interface TaskRunAttemptRecord {
  attempt_id: string;
  sequence: number;
  status: TaskRunAttemptStatus;
  started_at: string;
  finished_at?: string | null;
  recovery_reason?: string | null;
  model_response_id?: string | null;
}

export interface TaskRunRecord {
  id: string;
  task_id: string;
  model_config_id: string;
  memory_thread_id: string;
  status: TaskRunStatus;
  model_phase_status: ModelPhaseStatus;
  workspace_execution?: TaskRunWorkspaceExecution | null;
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
  attempt: number;
  attempts: TaskRunAttemptRecord[];
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
