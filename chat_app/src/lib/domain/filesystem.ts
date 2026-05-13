import type { FsEntryResponse, FsReadFileResponse } from '../api/client/types';
import type { FsEntry, FsReadResult } from '../../types';
import {
  asRecord,
  readBooleanFirst,
  readFirst,
  readNumberFirst,
  readStringFirst,
} from './normalizerUtils';

export interface NormalizeFsEntryOptions {
  fallbackIsDir?: boolean;
}

export const isValidEntryName = (name: string): boolean => (
  name !== '.'
  && name !== '..'
  && !name.includes('/')
  && !name.includes('\\')
  && !name.includes('\0')
);

export const normalizeFsEntry = (
  raw: FsEntryResponse | unknown,
  options: NormalizeFsEntryOptions = {},
): FsEntry => {
  const record = asRecord(raw);
  return {
    name: readStringFirst(record, ['name']),
    path: readStringFirst(record, ['path']),
    isDir: readBooleanFirst(record, ['is_dir', 'isDir'], options.fallbackIsDir ?? false),
    writable: (readFirst(record, ['writable']) ?? null) as FsEntry['writable'],
    size: (readFirst(record, ['size']) ?? null) as FsEntry['size'],
    modifiedAt: (readFirst(record, ['modified_at', 'modifiedAt']) ?? null) as FsEntry['modifiedAt'],
  };
};

export const normalizeFsReadResult = (raw: FsReadFileResponse | unknown): FsReadResult => {
  const record = asRecord(raw);
  return {
    path: readStringFirst(record, ['path']),
    name: readStringFirst(record, ['name']),
    size: readNumberFirst(record, ['size']),
    contentType: readStringFirst(record, ['content_type', 'contentType'], 'application/octet-stream'),
    isBinary: readBooleanFirst(record, ['is_binary', 'isBinary']),
    modifiedAt: (readFirst(record, ['modified_at', 'modifiedAt']) ?? null) as FsReadResult['modifiedAt'],
    content: readStringFirst(record, ['content']),
  };
};

export const deriveParentPath = (path: string): string | null => {
  const trimmed = path.trim();
  if (/^[A-Za-z]:[\\/]?$/.test(trimmed)) {
    return `${trimmed.slice(0, 2)}\\`;
  }
  const normalized = trimmed.replace(/[\\/]+$/, '');
  if (!normalized) return null;
  const idx = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'));
  if (idx < 0) return null;
  if (idx === 0) return normalized[0];
  const parent = normalized.slice(0, idx);
  if (/^[A-Za-z]:$/.test(parent)) {
    return `${parent}\\`;
  }
  return parent;
};
