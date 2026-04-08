import type { FsEntry } from '../../types';

export interface RemoteEntry {
  name: string;
  path: string;
  isDir: boolean;
  size?: number | null;
  modifiedAt?: string | null;
}

export interface SftpTransferStatus {
  id: string;
  direction: 'upload' | 'download';
  state: 'pending' | 'running' | 'cancelling' | 'success' | 'error' | 'cancelled';
  totalBytes: number | null;
  transferredBytes: number;
  percent: number | null;
  currentPath: string | null;
  message: string | null;
  error: string | null;
}

export interface SftpTransferRequest {
  id: string;
  direction: 'upload' | 'download';
  localSource: string;
  remoteSource: string;
  fallbackSuccess: string;
  label: string;
}

export type LocalEntry = FsEntry;
