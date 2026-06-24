export type AskUserPromptStatus = 'pending' | 'ok' | 'canceled' | 'timeout' | 'failed';

export interface AskUserPromptChoiceOption {
  value: string;
  label?: string;
  description?: string;
}

export interface AskUserPromptChoicePayload {
  multiple?: boolean;
  options?: AskUserPromptChoiceOption[];
  min_selections?: number;
  max_selections?: number;
  single_min_selections?: number;
  single_max_selections?: number;
  default?: string | string[] | null;
}

export interface AskUserPromptFieldPayload {
  key?: string;
  name?: string;
  label?: string;
  description?: string;
  placeholder?: string;
  default?: string;
  default_value?: string;
  required?: boolean;
  multiline?: boolean;
  secret?: boolean;
}

export interface AskUserPromptPayloadBody {
  fields?: AskUserPromptFieldPayload[];
  choice?: AskUserPromptChoicePayload;
  [key: string]: unknown;
}

export interface AskUserPromptStoredPrompt {
  prompt_id?: string;
  conversation_id?: string;
  conversation_turn_id?: string;
  tool_call_id?: string | null;
  kind?: string;
  title?: string;
  message?: string;
  allow_cancel?: boolean;
  timeout_ms?: number;
  payload?: AskUserPromptPayloadBody;
  source?: string;
  external_task_id?: string | null;
  external_run_id?: string | null;
  external_project_id?: string | null;
  [key: string]: unknown;
}

export interface AskUserPromptRecord {
  id: string;
  conversation_id: string;
  conversation_turn_id: string;
  tool_call_id?: string | null;
  kind: string;
  status: AskUserPromptStatus | string;
  prompt: AskUserPromptStoredPrompt;
  response?: unknown;
  expires_at?: string | null;
  source?: string;
  external_prompt_id?: string | null;
  external_task_id?: string | null;
  external_run_id?: string | null;
  external_project_id?: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface AskUserPromptListResponse {
  success?: boolean;
  conversation_id?: string;
  conversationId?: string;
  count?: number;
  prompts?: AskUserPromptRecord[];
  error?: string;
}

export interface AskUserPromptMutationPayload {
  conversation_id?: string;
  conversationId?: string;
  values?: Record<string, string>;
  selection?: string | string[];
  reason?: string;
}

export interface AskUserPromptMutationResponse {
  success?: boolean;
  prompt?: AskUserPromptRecord;
  task_runner_prompt?: unknown;
  error?: string;
}
