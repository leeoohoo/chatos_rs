export interface ChangeRecord {
  id: string;
  path: string;
  action: string;
  bytes: number;
  sha256: string;
  diff?: string | null;
  session_id: string;
  run_id: string;
  created_at: string;
}

export interface StatusResponse {
  ok: boolean;
  server_name: string;
  root: string;
  db_path: string;
  allow_writes: boolean;
  max_file_bytes: number;
  max_write_bytes: number;
  search_limit: number;
  session_id: string;
  run_id: string;
}

export interface FileResponse {
  ok: boolean;
  path: string;
  size_bytes: number;
  sha256: string;
  content: string;
}
