export const isRecord = (value: unknown): value is Record<string, unknown> => (
  typeof value === 'object' && value !== null && !Array.isArray(value)
);

export const readString = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const readStringArray = (value: unknown): string[] => (
  Array.isArray(value)
    ? value.map((item) => readString(item)).filter((item): item is string => Boolean(item))
    : []
);

export const formatDateTime = (value?: string | null): string => {
  const normalized = readString(value);
  if (!normalized) {
    return '-';
  }
  const date = new Date(normalized);
  if (Number.isNaN(date.getTime())) {
    return normalized;
  }
  return date.toLocaleString();
};

export const stringifyValue = (value: unknown): string => {
  if (value === null || value === undefined || value === '') {
    return '-';
  }
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
};

export const statusTone = (status?: string | null): string => {
  const normalized = readString(status)?.toLowerCase();
  if (normalized === 'succeeded' || normalized === 'completed' || normalized === 'success') {
    return 'border-emerald-200 bg-emerald-50 text-emerald-700';
  }
  if (normalized === 'failed' || normalized === 'error' || normalized === 'cancelled') {
    return 'border-red-200 bg-red-50 text-red-700';
  }
  if (normalized === 'running' || normalized === 'processing') {
    return 'border-sky-200 bg-sky-50 text-sky-700';
  }
  if (normalized === 'queued') {
    return 'border-amber-200 bg-amber-50 text-amber-700';
  }
  if (normalized === 'blocked') {
    return 'border-orange-200 bg-orange-50 text-orange-700';
  }
  return 'border-border bg-muted text-muted-foreground';
};

export const extractReportContent = (report: unknown): string | null => {
  if (!isRecord(report)) {
    return readString(report);
  }
  const direct = readString(report.content);
  if (direct) {
    return direct;
  }
  const output = readString(report.output);
  if (output) {
    return output;
  }
  return null;
};
