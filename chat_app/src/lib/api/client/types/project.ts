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
  cwd?: string;
  command?: string;
  source?: string;
  confidence?: number;
  is_default?: boolean;
  isDefault?: boolean;
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

export interface ProjectChangeLogResponse {
  id: string;
  server_name?: string;
  serverName?: string;
  path?: string;
  action?: string;
  change_kind?: 'create' | 'edit' | 'delete' | string;
  changeKind?: 'create' | 'edit' | 'delete' | string;
  bytes?: number;
  sha256?: string | null;
  diff?: string | null;
  conversation_id?: string | null;
  conversationId?: string | null;
  run_id?: string | null;
  runId?: string | null;
  confirmed?: boolean;
  confirmed_at?: string | null;
  confirmedAt?: string | null;
  confirmed_by?: string | null;
  confirmedBy?: string | null;
  created_at?: string;
  createdAt?: string;
  conversation_title?: string | null;
  conversationTitle?: string | null;
}

export interface ProjectChangeMarkResponse {
  path?: string;
  relative_path?: string;
  relativePath?: string;
  kind?: 'create' | 'edit' | 'delete' | string;
  last_change_id?: string;
  lastChangeId?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface ProjectChangeSummaryResponse {
  file_marks?: ProjectChangeMarkResponse[];
  fileMarks?: ProjectChangeMarkResponse[];
  deleted_marks?: ProjectChangeMarkResponse[];
  deletedMarks?: ProjectChangeMarkResponse[];
  counts?: {
    create?: number;
    edit?: number;
    delete?: number;
    total?: number;
  };
}

export interface ProjectChangeConfirmResponse {
  confirmed?: number;
  requested?: number;
  mode?: 'all' | 'paths' | 'change_ids' | string;
}
