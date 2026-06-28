export interface ToolingNoteSummary {
  id: string;
  title: string;
  folder: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  file: string;
}

export interface ToolingNotepadFoldersResponse {
  ok: boolean;
  folders: string[];
}

export interface ToolingNotepadNotesResponse {
  ok: boolean;
  notes: ToolingNoteSummary[];
}

export interface ToolingNotepadNoteResponse {
  ok: boolean;
  note: ToolingNoteSummary;
  content: string;
}

export interface ToolingTagCount {
  tag: string;
  count: number;
}

export interface ToolingNotepadTagsResponse {
  ok: boolean;
  tags: ToolingTagCount[];
}

export interface ToolingTerminalLogEntry {
  offset: number;
  kind: string;
  content: string;
  created_at: string;
}

export interface ToolingTerminalProcessRecord {
  terminal_id: string;
  process_id: string;
  terminal_name: string;
  status: string;
  process_status: string;
  busy: boolean;
  has_session: boolean;
  command: string;
  pid?: number | null;
  started_at: string;
  uptime_seconds?: number | null;
  cwd: string;
  project_id?: string | null;
  last_active_at: string;
  output_preview: string;
  output_tail: string;
  output_tail_chars: number;
  exit_code?: number | null;
}

export interface ToolingTerminalProcessListResponse {
  status: string;
  result_scope: string;
  is_multiple_terminals: boolean;
  terminal_count: number;
  process_count: number;
  visible_total: number;
  total_terminals: number;
  include_exited: boolean;
  limit: number;
  terminals: ToolingTerminalProcessRecord[];
  processes: ToolingTerminalProcessRecord[];
}

export interface ToolingTerminalProcessLogsResponse {
  terminal_id: string;
  process_id: string;
  terminal_name: string;
  status: string;
  process_status: string;
  busy: boolean;
  has_session: boolean;
  command: string;
  pid?: number | null;
  started_at: string;
  uptime_seconds?: number | null;
  cwd: string;
  project_id?: string | null;
  last_active_at: string;
  mode: string;
  requested_offset?: number | null;
  next_offset?: number | null;
  limit: number;
  fetched_log_count: number;
  returned_log_count: number;
  has_more: boolean;
  truncated: boolean;
  logs: ToolingTerminalLogEntry[];
  output_preview: string;
  output_tail: string;
  output_tail_chars: number;
  exit_code?: number | null;
}

export interface ToolingTerminalKillResponse {
  ok: boolean;
  terminal_id: string;
  killed: boolean;
}

export interface ToolingTerminalWriteResponse {
  ok: boolean;
  terminal_id: string;
  bytes_written: number;
  submit: boolean;
}
