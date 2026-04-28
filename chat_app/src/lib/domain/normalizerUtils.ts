export type UnknownRecord = Record<string, unknown>;

export const asRecord = (value: unknown): UnknownRecord | null => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

export const readValue = (record: UnknownRecord | null, key: string): unknown => (
  record?.[key]
);

export const readString = (record: UnknownRecord | null, key: string): string => {
  const value = readValue(record, key);
  return typeof value === 'string' ? value : '';
};

export const readTrimmedString = (record: UnknownRecord | null, key: string): string => (
  readString(record, key).trim()
);

export const readBoolean = (record: UnknownRecord | null, key: string): boolean | undefined => {
  const value = readValue(record, key);
  return typeof value === 'boolean' ? value : undefined;
};

export const normalizeDate = (value: unknown): Date => {
  if (typeof value === 'string' || typeof value === 'number' || value instanceof Date) {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return new Date();
};
