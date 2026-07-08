// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type UnknownRecord = Record<string, unknown>;

export const asRecord = (value: unknown): UnknownRecord | null => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

export const readValue = (record: UnknownRecord | null, key: string): unknown => (
  record?.[key]
);

export const readFirst = (
  record: UnknownRecord | null,
  keys: string[],
): unknown => {
  for (const key of keys) {
    const value = readValue(record, key);
    if (value !== undefined) {
      return value;
    }
  }
  return undefined;
};

export const readString = (
  record: UnknownRecord | null,
  key: string,
  fallback = '',
): string => {
  const value = readValue(record, key);
  return typeof value === 'string' ? value : fallback;
};

export const readTrimmedString = (record: UnknownRecord | null, key: string): string => (
  readString(record, key).trim()
);

export const readStringFirst = (
  record: UnknownRecord | null,
  keys: string[],
  fallback = '',
): string => {
  const value = readFirst(record, keys);
  return typeof value === 'string' ? value : fallback;
};

export const readNullableStringFirst = (
  record: UnknownRecord | null,
  keys: string[],
): string | null => {
  const value = readFirst(record, keys);
  return typeof value === 'string' ? value : null;
};

export const readNumberFirst = (
  record: UnknownRecord | null,
  keys: string[],
  fallback = 0,
): number => {
  const value = Number(readFirst(record, keys));
  return Number.isFinite(value) ? value : fallback;
};

export const readBoolean = (record: UnknownRecord | null, key: string): boolean | undefined => {
  const value = readValue(record, key);
  return typeof value === 'boolean' ? value : undefined;
};

export const readBooleanFirst = (
  record: UnknownRecord | null,
  keys: string[],
  fallback = false,
): boolean => (
  Boolean(readFirst(record, keys) ?? fallback)
);

export const normalizeDate = (value: unknown): Date => {
  if (typeof value === 'string' || typeof value === 'number' || value instanceof Date) {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return new Date();
};
