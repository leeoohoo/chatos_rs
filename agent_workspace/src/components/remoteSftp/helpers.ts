import type ApiClient from '../../lib/api/client';
import type {
  FsEntryResponse,
  FsEntriesResponse,
  RemoteSftpEntryResponse,
  RemoteSftpEntriesResponse,
  RemoteSftpTransferStatusResponse,
} from '../../lib/api/client/types';
import type { FsEntry } from '../../types';

import type { RemoteEntry, SftpTransferStatus } from './types';

export const normalizeLocalEntry = (raw: FsEntryResponse): FsEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

export const normalizeRemoteEntry = (raw: RemoteSftpEntryResponse): RemoteEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

export const normalizeTransferStatus = (
  raw: RemoteSftpTransferStatusResponse,
): SftpTransferStatus => ({
  id: raw?.id ?? '',
  direction: raw?.direction === 'download' ? 'download' : 'upload',
  state: (raw?.state ?? 'pending') as SftpTransferStatus['state'],
  totalBytes: raw?.total_bytes ?? raw?.totalBytes ?? null,
  transferredBytes: Number(raw?.transferred_bytes ?? raw?.transferredBytes ?? 0),
  percent: typeof raw?.percent === 'number' ? raw.percent : null,
  currentPath: raw?.current_path ?? raw?.currentPath ?? null,
  message: raw?.message ?? null,
  error: raw?.error ?? null,
});

export const normalizeFsEntriesResponse = (data: FsEntriesResponse) => ({
  path: data?.path ?? null,
  parent: data?.parent ?? null,
  entries: Array.isArray(data?.entries) ? data.entries.map(normalizeLocalEntry) : [],
  roots: Array.isArray(data?.roots) ? data.roots.map(normalizeLocalEntry) : [],
});

export const normalizeRemoteEntriesResponse = (data: RemoteSftpEntriesResponse) => ({
  path: data?.path ?? '.',
  parent: data?.parent ?? null,
  entries: Array.isArray(data?.entries) ? data.entries.map(normalizeRemoteEntry) : [],
});

export const formatBytes = (value: number): string => {
  if (!Number.isFinite(value) || value <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = value;
  let idx = 0;
  while (size >= 1024 && idx < units.length - 1) {
    size /= 1024;
    idx += 1;
  }
  return `${size.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
};

export const joinLocalPath = (base: string, name: string): string => {
  const normalized = base.replace(/[\\/]+$/, '');
  if (!normalized) return name;
  const sep = normalized.includes('\\') ? '\\' : '/';
  return `${normalized}${sep}${name}`;
};

export const joinRemotePath = (base: string, name: string): string => {
  const normalized = base.replace(/\/+$/, '');
  if (!normalized || normalized === '.') return name;
  if (normalized === '/') return `/${name}`;
  return `${normalized}/${name}`;
};

export const remoteDirname = (path: string): string => {
  const normalized = path.trim().replace(/\/+$/, '');
  if (!normalized || normalized === '.' || normalized === '/') return '.';
  const idx = normalized.lastIndexOf('/');
  if (idx < 0) return '.';
  if (idx === 0) return '/';
  return normalized.slice(0, idx);
};

export type RemoteSftpClient = Pick<
  ApiClient,
  | 'listFsEntries'
  | 'listRemoteSftpEntries'
  | 'startRemoteSftpTransfer'
  | 'getRemoteSftpTransferStatus'
  | 'cancelRemoteSftpTransfer'
  | 'createRemoteSftpDirectory'
  | 'renameRemoteSftpEntry'
  | 'deleteRemoteSftpEntry'
>;
