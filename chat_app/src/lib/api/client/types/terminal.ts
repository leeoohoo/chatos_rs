// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface TerminalResponse {
  id: string;
  name?: string;
  cwd?: string;
  display_cwd?: string | null;
  displayCwd?: string | null;
  kind?: string | null;
  user_id?: string | null;
  userId?: string | null;
  project_id?: string | null;
  projectId?: string | null;
  status?: string;
  busy?: boolean;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  last_active_at?: string;
  lastActiveAt?: string;
}

export interface TerminalLogResponse {
  id: string;
  terminal_id?: string;
  terminalId?: string;
  log_type?: string;
  logType?: string;
  type?: string;
  content?: string;
  created_at?: string;
  createdAt?: string;
}

export interface TerminalDispatchResponse {
  success?: boolean;
  terminal_id?: string;
  terminalId?: string;
  terminal_name?: string;
  terminalName?: string;
  terminal_reused?: boolean;
  terminalReused?: boolean;
  status?: string;
  interrupted?: boolean;
  signal?: string;
  reason?: string;
  cwd?: string;
  executed_command?: string;
  executedCommand?: string;
  project_id?: string;
  projectId?: string;
  message?: string;
}
