export type QueryPrimitive = string | number | boolean | null | undefined;
export type JsonRecord = Record<string, unknown>;

export interface ParsedJsonErrorPayload {
  message: string;
  code?: string;
  payload: unknown;
}

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code?: string;
  readonly payload?: unknown;

  constructor(message: string, options?: { status?: number; code?: string; payload?: unknown }) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = options?.status ?? 0;
    this.code = options?.code;
    this.payload = options?.payload;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export const isJsonRecord = (value: unknown): value is JsonRecord => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
);

export const parseJsonText = (text: string): unknown => {
  if (!text) {
    return null;
  }
  return JSON.parse(text) as unknown;
};

export const parseJsonTextSafely = (text: string): { parsed: unknown; ok: boolean } => {
  if (!text) {
    return { parsed: null, ok: false };
  }
  try {
    return {
      parsed: parseJsonText(text),
      ok: true,
    };
  } catch {
    return {
      parsed: null,
      ok: false,
    };
  }
};

export const getErrorMessageFromPayload = (
  payload: unknown,
  fallback: string,
): string => {
  if (!isJsonRecord(payload)) {
    return fallback;
  }
  const errorValue = payload.error;
  if (typeof errorValue === 'string' && errorValue.trim().length > 0) {
    return errorValue;
  }
  const messageValue = payload.message;
  if (typeof messageValue === 'string' && messageValue.trim().length > 0) {
    return messageValue;
  }
  return fallback;
};

export const getErrorCodeFromPayload = (payload: unknown): string | undefined => {
  if (!isJsonRecord(payload)) {
    return undefined;
  }
  return typeof payload.code === 'string' ? payload.code : undefined;
};

export const buildParsedJsonErrorPayload = (
  text: string,
  fallbackMessage: string,
): ParsedJsonErrorPayload => {
  if (!text) {
    return {
      message: fallbackMessage,
      code: undefined,
      payload: null,
    };
  }

  const { parsed, ok } = parseJsonTextSafely(text);
  if (!ok) {
    return {
      message: text,
      code: undefined,
      payload: null,
    };
  }

  return {
    message: getErrorMessageFromPayload(parsed, fallbackMessage),
    code: getErrorCodeFromPayload(parsed),
    payload: parsed,
  };
};

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
