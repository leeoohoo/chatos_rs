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

export const asStringList = (value: unknown): string[] => (
  asArray(value)
    .map((item) => asString(item).trim())
    .filter((item) => item.length > 0)
);
