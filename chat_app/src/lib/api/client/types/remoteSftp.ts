export interface RemoteSftpEntryResponse {
  name?: string;
  path?: string;
  is_dir?: boolean;
  isDir?: boolean;
  size?: number | null;
  modified_at?: string | null;
  modifiedAt?: string | null;
}

export interface RemoteSftpEntriesResponse {
  path?: string | null;
  parent?: string | null;
  entries?: RemoteSftpEntryResponse[];
}

export interface RemoteSftpTransferStatusResponse {
  id?: string;
  direction?: 'upload' | 'download';
  state?: 'pending' | 'running' | 'cancelling' | 'success' | 'error' | 'cancelled' | string;
  total_bytes?: number | null;
  totalBytes?: number | null;
  transferred_bytes?: number;
  transferredBytes?: number;
  percent?: number | null;
  current_path?: string | null;
  currentPath?: string | null;
  message?: string | null;
  error?: string | null;
}

export interface SftpTransferStartPayload {
  direction: 'upload' | 'download';
  local_path: string;
  remote_path: string;
}
