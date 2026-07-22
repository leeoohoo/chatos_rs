// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const containsAny = (value: string, candidates: string[]): boolean => (
  candidates.some((candidate) => value.includes(candidate))
);

export const isTransientServiceAppError = (error: string | null | undefined): boolean => {
  const normalized = String(error || '').trim();
  const lower = normalized.toLowerCase();
  return Boolean(normalized) && (
    /status\s+5\d\d\b/i.test(normalized)
    || containsAny(lower, [
      'failed to fetch',
      'error sending request for url',
      'connection refused',
      'connection reset',
      'network is unreachable',
      'service unavailable',
      'bad gateway',
      'gateway timeout',
      'timed out',
      'timeout',
      'user_service 鉴权失败',
    ])
  );
};

export const sanitizeUserVisibleAppError = (error: string): string => {
  const normalized = String(error || '').trim();
  if (!normalized) {
    return '请求失败，请稍后重试。';
  }
  const lower = normalized.toLowerCase();

  if (isTransientServiceAppError(normalized)) {
    return '服务暂时不可用，请稍后重试。';
  }

  if (containsAny(lower, [
    'api_key',
    'api key',
    'authorization',
    'bearer ',
    'access_token',
    'internal_trace',
  ])) {
    return '请求失败，请稍后重试。';
  }

  if (
    normalized.length > 240
    || (normalized.includes('{') && normalized.includes('}'))
    || normalized.includes('trace=')
  ) {
    return '请求失败，请稍后重试。';
  }

  return normalized;
};
