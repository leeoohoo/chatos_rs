export function textOrUndefined(value?: string | null): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

export function numberOrNull(value?: number | null): number | null {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return null;
  }
  return value;
}

export function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
