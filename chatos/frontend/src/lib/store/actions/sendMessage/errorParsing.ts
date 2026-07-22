// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

type ErrorRecord = Record<string, unknown>;

const asErrorRecord = (value: unknown): ErrorRecord | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as ErrorRecord
    : null
);

const tryParseJsonObject = (raw: string): ErrorRecord | null => {
  const trimmed = raw.trim();
  if (!trimmed.startsWith('{') || !trimmed.endsWith('}')) {
    return null;
  }
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as ErrorRecord;
    }
  } catch {
    return null;
  }
  return null;
};

export const resolveNestedErrorDetails = (
  candidate: unknown,
  depth = 0,
): { message?: string; code?: string } => {
  if (depth > 4 || candidate === null || candidate === undefined) {
    return {};
  }

  if (candidate instanceof Error) {
    return resolveNestedErrorDetails(candidate.message, depth + 1);
  }

  if (typeof candidate === 'string') {
    const trimmed = candidate.trim();
    if (!trimmed) {
      return {};
    }
    const parsed = tryParseJsonObject(trimmed);
    if (parsed) {
      const fromParsed = resolveNestedErrorDetails(parsed, depth + 1);
      if (fromParsed.message || fromParsed.code) {
        return fromParsed;
      }
    }
    return { message: trimmed };
  }

  if (typeof candidate !== 'object') {
    return {};
  }

  const raw = asErrorRecord(candidate);
  if (!raw) {
    return {};
  }
  const directMessage = typeof raw.message === 'string' ? raw.message.trim() : '';
  const directCode = typeof raw.code === 'string'
    ? raw.code.trim()
    : (typeof raw.type === 'string' ? raw.type.trim() : '');

  if (directMessage) {
    return {
      message: directMessage,
      code: directCode || undefined,
    };
  }

  const nestedCandidates = [raw.error, raw.data, raw.details];
  for (const nested of nestedCandidates) {
    const resolved = resolveNestedErrorDetails(nested, depth + 1);
    if (resolved.message || resolved.code) {
      if (!resolved.code && directCode) {
        return { ...resolved, code: directCode };
      }
      return resolved;
    }
  }

  return directCode ? { code: directCode } : {};
};

export const resolveStreamErrorPayload = (
  payload: unknown,
): { message: string; code?: string } => {
  const source = asErrorRecord(payload) || {};
  const data = asErrorRecord(source.data) || {};
  const directCode = typeof source.code === 'string'
    ? source.code.trim()
    : (typeof data.code === 'string' ? data.code.trim() : '');

  const candidates = [
    source.message,
    source.error,
    data.message,
    data.error,
  ];

  for (const candidate of candidates) {
    const resolved = resolveNestedErrorDetails(candidate);
    if (resolved.message) {
      return {
        message: resolved.message,
        code: directCode || resolved.code,
      };
    }
  }

  const fallbackResolved = resolveNestedErrorDetails(payload);
  if (fallbackResolved.message) {
    return {
      message: fallbackResolved.message,
      code: directCode || fallbackResolved.code,
    };
  }

  return {
    message: 'Stream error',
    code: directCode || fallbackResolved.code,
  };
};

export const resolveReadableErrorMessage = (inputError: unknown): string => {
  const nested = resolveNestedErrorDetails(inputError);
  if (typeof nested.message === 'string' && nested.message.trim().length > 0) {
    return sanitizeUserVisibleAiError(nested.message);
  }
  if (inputError instanceof Error && inputError.message.trim().length > 0) {
    return sanitizeUserVisibleAiError(inputError.message);
  }
  if (typeof inputError === 'string' && inputError.trim().length > 0) {
    return sanitizeUserVisibleAiError(inputError);
  }
  const raw = asErrorRecord(inputError);
  if (raw && typeof raw.message === 'string' && raw.message.trim().length > 0) {
    return sanitizeUserVisibleAiError(raw.message);
  }
  return '请求失败，请稍后重试';
};

const includesAny = (value: string, needles: string[]): boolean => (
  needles.some((needle) => value.includes(needle))
);

export const sanitizeUserVisibleAiError = (reason: string): string => {
  const normalized = String(reason || '').trim();
  if (!normalized) {
    return '请求失败，请稍后重试';
  }

  const lower = normalized.toLowerCase();
  if (
    lower === 'chat turn cancelled'
    || lower === 'chat turn canceled'
    || lower === 'cancelled'
    || lower === 'canceled'
    || lower === 'aborted'
  ) {
    return '已停止生成';
  }

  if (includesAny(lower, [
    'stream response parse failed',
    'invalid json response',
    'error decoding response body',
    'no valid sse events',
  ])) {
    const retries = normalized.match(/已重试\s*(\d+)\s*次/);
    return retries
      ? `模型服务响应异常，已自动重试 ${retries[1]} 次，请稍后重试或切换模型。`
      : '模型服务响应异常，请稍后重试或切换模型。';
  }

  if (includesAny(lower, [
    'api_key',
    'api key',
    'authorization',
    'bearer ',
    'access_token',
    'internal_trace',
    'provider_error',
  ])) {
    return '模型服务调用失败，请稍后重试或检查模型配置。';
  }

  if (
    /status\s+5\d\d\b/i.test(normalized)
    || includesAny(lower, [
      'internal server error',
      'bad gateway',
      'service unavailable',
      'gateway timeout',
      'error sending request for url',
      'connection reset',
      'connection refused',
      'network is unreachable',
      'timed out',
      'timeout',
    ])
  ) {
    return '模型服务暂时不可用，请稍后重试或切换模型。';
  }

  if ((normalized.includes('{') && normalized.includes('}')) || normalized.includes('trace=')) {
    return '模型服务调用失败，请稍后重试或检查模型配置。';
  }

  return normalized;
};

export const formatAssistantFailureContent = (reason: string, existingContent: string): string => {
  const normalizedReason = sanitizeUserVisibleAiError(reason);
  if (existingContent.trim().length > 0) {
    return `${existingContent.trim()}\n\n[请求失败] ${normalizedReason}`;
  }
  return `请求失败：${normalizedReason}`;
};
