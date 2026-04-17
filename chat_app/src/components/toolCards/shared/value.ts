export const asRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

export const asString = (value: unknown): string => (
  typeof value === 'string' ? value : ''
);

export const asBoolean = (value: unknown): boolean | null => (
  typeof value === 'boolean' ? value : null
);

export const asNumber = (value: unknown): number | null => {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'string' && value.trim().length > 0) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return null;
};

export const asArray = (value: unknown): unknown[] => (
  Array.isArray(value) ? value : []
);

export const formatDateTime = (value: unknown): string => {
  const numeric = asNumber(value);
  if (numeric !== null) {
    const date = new Date(numeric);
    if (!Number.isNaN(date.getTime())) {
      return date.toLocaleString();
    }
  }
  const text = asString(value).trim();
  if (!text) {
    return '';
  }
  const parsed = new Date(text);
  return Number.isNaN(parsed.getTime()) ? text : parsed.toLocaleString();
};

export const buildLineRangeLabel = (
  startLine: number | null,
  endLine: number | null,
): string => {
  if (startLine !== null && endLine !== null) {
    return `${startLine}-${endLine}`;
  }
  if (startLine !== null) {
    return `from ${startLine}`;
  }
  if (endLine !== null) {
    return `to ${endLine}`;
  }
  return '';
};

