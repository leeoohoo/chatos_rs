const tryParseJsonObject = (raw: string): Record<string, any> | null => {
  const trimmed = raw.trim();
  if (!trimmed.startsWith('{') || !trimmed.endsWith('}')) {
    return null;
  }
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, any>;
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

  const raw = candidate as Record<string, any>;
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

export const resolveStreamErrorPayload = (payload: any): { message: string; code?: string } => {
  const directCode = typeof payload?.code === 'string'
    ? payload.code.trim()
    : (typeof payload?.data?.code === 'string' ? payload.data.code.trim() : '');

  const candidates = [
    payload?.message,
    payload?.error,
    payload?.data?.message,
    payload?.data?.error,
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
    return nested.message.trim();
  }
  if (inputError instanceof Error && inputError.message.trim().length > 0) {
    return inputError.message.trim();
  }
  if (typeof inputError === 'string' && inputError.trim().length > 0) {
    return inputError.trim();
  }
  if (inputError && typeof inputError === 'object') {
    const maybeMessage = (inputError as any).message;
    if (typeof maybeMessage === 'string' && maybeMessage.trim().length > 0) {
      return maybeMessage.trim();
    }
  }
  return '请求失败，请稍后重试';
};

export const formatAssistantFailureContent = (reason: string, existingContent: string): string => {
  const normalizedReason = reason.trim().length > 0 ? reason.trim() : '请求失败，请稍后重试';
  if (existingContent.trim().length > 0) {
    return `${existingContent.trim()}\n\n[请求失败] ${normalizedReason}`;
  }
  return `请求失败：${normalizedReason}`;
};
