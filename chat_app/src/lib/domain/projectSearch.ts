import type { FsContentSearchEntryResponse } from '../api/client/types';
import type { ProjectSearchHit } from '../../types';

type UnknownRecord = Record<string, unknown>;

const asRecord = (value: unknown): UnknownRecord | null => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

const readValue = (record: UnknownRecord | null, key: string): unknown => record?.[key];

const readString = (record: UnknownRecord | null, key: string, fallback = ''): string => {
  const value = readValue(record, key);
  return typeof value === 'string' ? value : fallback;
};

const readFirst = (record: UnknownRecord | null, keys: string[]): unknown => {
  for (const key of keys) {
    const value = readValue(record, key);
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

export const normalizeProjectSearchHit = (
  raw: FsContentSearchEntryResponse | unknown,
): ProjectSearchHit => {
  const record = asRecord(raw);
  const path = readString(record, 'path');
  return {
    path,
    relativePath: readStringFirst(record, ['relative_path', 'relativePath'], path),
    line: readNumberFirst(record, ['line'], 1),
    column: readNumberFirst(record, ['column'], 1),
    text: readString(record, 'text'),
  };
};

export const buildProjectSearchHitId = (hit: ProjectSearchHit): string => (
  `${hit.path}:${hit.line}:${hit.column}`
);
