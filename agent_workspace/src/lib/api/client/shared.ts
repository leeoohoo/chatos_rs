export type QueryPrimitive = string | number | boolean | null | undefined;

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code?: string;
  readonly payload?: any;

  constructor(message: string, options?: { status?: number; code?: string; payload?: any }) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = options?.status ?? 0;
    this.code = options?.code;
    this.payload = options?.payload;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export const buildQuery = (params: Record<string, QueryPrimitive>): string => {
  const search = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value === undefined || value === null) {
      return;
    }
    if (typeof value === 'string' && value.trim().length === 0) {
      return;
    }
    if (typeof value === 'boolean') {
      search.set(key, value ? 'true' : 'false');
      return;
    }
    search.set(key, String(value));
  });
  const query = search.toString();
  return query ? `?${query}` : '';
};

export const parseFilenameFromContentDisposition = (value: string | null): string | null => {
  if (!value) return null;

  const utf8Match = /filename\*\s*=\s*UTF-8''([^;]+)/i.exec(value);
  if (utf8Match?.[1]) {
    try {
      return decodeURIComponent(utf8Match[1]);
    } catch {
      // ignore decode error
    }
  }

  const plainMatch = /filename\s*=\s*"([^"]+)"|filename\s*=\s*([^;]+)/i.exec(value);
  const name = plainMatch?.[1] || plainMatch?.[2] || '';
  const trimmed = name.trim();
  if (!trimmed) return null;
  return trimmed;
};

export const guessFilenameFromPath = (path: string): string => {
  const trimmed = (path || '').trim().replace(/[\\/]+$/, '');
  if (!trimmed) return 'download';
  const idx = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
  if (idx < 0) return trimmed;
  return trimmed.slice(idx + 1) || 'download';
};
