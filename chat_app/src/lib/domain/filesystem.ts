import type { FsEntryResponse, FsReadFileResponse } from '../api/client/types';
import type { FsEntry, FsReadResult } from '../../types';

type UnknownRecord = Record<string, unknown>;

const asRecord = (value: unknown): UnknownRecord | null => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

const readFirst = (record: UnknownRecord | null, keys: string[]): unknown => {
  for (const key of keys) {
    const value = record?.[key];
    if (value !== undefined) {
      return value;
    }
  }
  return undefined;
};

const readStringFirst = (record: UnknownRecord | null, keys: string[], fallback = ''): string => {
  const value = readFirst(record, keys);
  return typeof value === 'string' ? value : fallback;
};

const readNumberFirst = (record: UnknownRecord | null, keys: string[], fallback = 0): number => {
  const value = Number(readFirst(record, keys));
  return Number.isFinite(value) ? value : fallback;
};

const readBooleanFirst = (record: UnknownRecord | null, keys: string[], fallback = false): boolean => (
  Boolean(readFirst(record, keys) ?? fallback)
);

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
