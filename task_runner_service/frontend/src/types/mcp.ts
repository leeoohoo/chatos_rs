import type { TaskBuiltinPromptMode, TaskMcpInitMode } from './tasks';

export interface McpUnavailableTool {
  name: string;
  reason: string;
}

export interface McpCatalogEntry {
  kind: string;
  server_name: string;
  config_id?: string | null;
  command?: string | null;
  description: string;
  use_cases: string[];
  capabilities: string[];
  implemented: boolean;
  runtime_default: boolean;
  default_allow_writes: boolean;
  available_tool_names: string[];
  unavailable_tools: McpUnavailableTool[];
  message?: string | null;
}

export interface McpServerInfo {
  server_name: string;
  transports: string[];
  http_endpoint_path?: string | null;
  stdio_command?: string | null;
  stdio_args: string[];
  tool_names: string[];
  tool_profiles?: McpServerToolProfileInfo[];
}

export interface McpServerToolProfileInfo {
  key: string;
  label: string;
  description: string;
  tool_names: string[];
}

export interface TaskRunnerSkillResponse {
  name: string;
  locale: string;
  content: string;
}

export interface TaskRunnerInternalPromptPreviewResponse {
  locale: string;
  task_prompt_template: string;
  global_execution_prompt: string;
  process_log_system_prompt: string;
  notes: string[];
}

export interface McpPromptPreviewPayload {
  enabled?: boolean;
  init_mode?: TaskMcpInitMode;
  builtin_prompt_mode?: TaskBuiltinPromptMode;
  builtin_prompt_locale?: string;
  enabled_builtin_kinds?: string[];
  workspace_dir?: string;
  default_remote_server_id?: string;
}

export interface McpPromptBuildResult {
  prompt?: string | null;
  selected_section_ids: string[];
  omitted_section_ids: string[];
  requested_builtin_server_names: string[];
  active_builtin_server_names: string[];
  omitted_builtin_server_names: string[];
  runtime_limitations?: string | null;
}

export interface McpPromptPreviewResponse {
  enabled: boolean;
  init_mode: TaskMcpInitMode;
  builtin_prompt_mode: TaskBuiltinPromptMode;
  builtin_prompt_locale: string;
  selected_builtin_kinds: string[];
  build: McpPromptBuildResult;
}
