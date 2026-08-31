// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type AskUserPromptStatus =
  | 'pending'
  | 'submitted'
  | 'cancelled'
  | 'timed_out'
  | 'failed';

export interface AskUserPromptTaskCountRecord {
  task_id: string;
  count: number;
}

export interface AskUserPromptResponseSubmission {
  status: string;
  values?: unknown;
  selection?: unknown;
  reason?: string | null;
}

export interface AskUserPromptRecord {
  id: string;
  task_id?: string | null;
  run_id?: string | null;
  conversation_id: string;
  conversation_turn_id: string;
  tool_call_id?: string | null;
  kind: string;
  title: string;
  message: string;
  allow_cancel: boolean;
  timeout_ms: number;
  payload: unknown;
  response?: AskUserPromptResponseSubmission | null;
  status: AskUserPromptStatus;
  created_at: string;
  updated_at: string;
  expires_at?: string | null;
}

export interface PromptListFilters {
  taskId?: string;
  runId?: string;
  status?: AskUserPromptStatus;
  limit?: number;
  offset?: number;
}

export interface SubmitAskUserPromptPayload {
  values?: unknown;
  selection?: unknown;
  reason?: string;
}

export interface CancelAskUserPromptPayload {
  reason?: string;
}
