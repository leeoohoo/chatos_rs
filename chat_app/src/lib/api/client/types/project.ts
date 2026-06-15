export interface ProjectResponse {
  id: string;
  name: string;
  root_path?: string;
  rootPath?: string;
  description?: string | null;
  user_id?: string | null;
  userId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface ProjectRunTargetResponse {
  id: string;
  label?: string;
  kind?: string;
  language?: string | null;
  cwd?: string;
  command?: string;
  source?: string;
  confidence?: number;
  is_default?: boolean;
  isDefault?: boolean;
  entrypoint?: string | null;
  entry_point?: string | null;
  manifest_path?: string | null;
  manifestPath?: string | null;
  required_toolchains?: string[];
  requiredToolchains?: string[];
}

export interface ProjectRunCatalogResponse {
  project_id?: string;
  projectId?: string;
  status?: string;
  default_target_id?: string | null;
  defaultTargetId?: string | null;
  targets?: ProjectRunTargetResponse[];
  error_message?: string | null;
  errorMessage?: string | null;
  analyzed_at?: string | null;
  analyzedAt?: string | null;
  updated_at?: string | null;
  updatedAt?: string | null;
}

export interface ProjectRunExecuteResponse {
  success?: boolean;
  status?: string;
  run_id?: string;
  runId?: string;
  terminal_id?: string;
  terminalId?: string;
  target_id?: string;
  targetId?: string;
  command?: string;
  cwd?: string;
  message?: string;
  error?: string;
  env_overrides?: Record<string, string>;
  envOverrides?: Record<string, string>;
}

export interface ProjectRunStateResponse {
  project_id?: string;
  projectId?: string;
  running?: boolean;
  busy?: boolean;
  status?: string;
  terminal_id?: string | null;
  terminalId?: string | null;
  terminal_name?: string | null;
  terminalName?: string | null;
  cwd?: string | null;
  terminal?: import('./terminal').TerminalResponse | null;
  instances?: Array<{
    terminal_id?: string | null;
    terminalId?: string | null;
    terminal_name?: string | null;
    terminalName?: string | null;
    cwd?: string | null;
    status?: string;
    busy?: boolean;
    running?: boolean;
    terminal?: import('./terminal').TerminalResponse | null;
  }>;
}

export interface ProjectRunToolchainOptionResponse {
  id: string;
  kind?: string;
  label?: string;
  version?: string | null;
  path?: string;
  source?: string;
  is_default?: boolean;
  isDefault?: boolean;
}

export interface ProjectRunConfigFileSummaryResponse {
  kind?: string;
  label?: string;
  path?: string;
  preview?: string | null;
  source?: string;
}

export interface ProjectRunValidationIssueResponse {
  kind?: string;
  message?: string;
  target_id?: string | null;
  targetId?: string | null;
  target_label?: string | null;
  targetLabel?: string | null;
  path?: string | null;
  hint?: string | null;
}

export interface ProjectRunCustomToolchainResponse {
  kind?: string;
  label?: string;
  path?: string;
}

export interface ProjectRunEnvironmentResponse {
  project_id?: string;
  projectId?: string;
  user_id?: string | null;
  userId?: string | null;
  options_by_kind?: Record<string, ProjectRunToolchainOptionResponse[]>;
  optionsByKind?: Record<string, ProjectRunToolchainOptionResponse[]>;
  config_files?: ProjectRunConfigFileSummaryResponse[];
  configFiles?: ProjectRunConfigFileSummaryResponse[];
  validation_issues?: ProjectRunValidationIssueResponse[];
  validationIssues?: ProjectRunValidationIssueResponse[];
  selected_toolchains?: Record<string, string>;
  selectedToolchains?: Record<string, string>;
  custom_toolchains?: Record<string, ProjectRunCustomToolchainResponse>;
  customToolchains?: Record<string, ProjectRunCustomToolchainResponse>;
  env_vars?: Record<string, string>;
  envVars?: Record<string, string>;
  terminal_ui_enabled?: boolean;
  terminalUiEnabled?: boolean;
  updated_at?: string | null;
  updatedAt?: string | null;
}

export interface ProjectContactLinkResponse {
  contact_id?: string;
  contactId?: string;
  agent_id?: string;
  agentId?: string;
  agent_name_snapshot?: string | null;
  agentNameSnapshot?: string | null;
  updated_at?: string | null;
  updatedAt?: string | null;
}
