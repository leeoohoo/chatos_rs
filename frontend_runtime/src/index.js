// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export function normalizeApiBaseUrl(value, options = {}) {
  const { stripApiSuffix = true } = options;
  let normalized = String(value ?? '').trim().replace(/\/+$/, '');
  if (stripApiSuffix) {
    normalized = normalized.replace(/\/api$/, '');
  }
  return normalized;
}

export function buildApiUrl(baseUrl, path) {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  return baseUrl ? `${baseUrl}${normalizedPath}` : normalizedPath;
}

export function withQuery(path, params) {
  const search = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value === undefined || value === null) {
      return;
    }
    const text = String(value).trim();
    if (text) {
      search.set(key, text);
    }
  });
  const query = search.toString();
  return query ? `${path}?${query}` : path;
}

function parseDateTime(value) {
  if (value instanceof Date) {
    return value;
  }
  const localDateTime = String(value).match(
    /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{1,2})(?::(\d{1,2})(?::(\d{1,2})(?:\.(\d+))?)?)?)?$/,
  );
  if (!localDateTime) {
    return new Date(value);
  }
  const [, year, month, day, hour = '0', minute = '0', second = '0', fraction = ''] =
    localDateTime;
  return new Date(
    Number(year),
    Number(month) - 1,
    Number(day),
    Number(hour),
    Number(minute),
    Number(second),
    Number(fraction.padEnd(3, '0').slice(0, 3)),
  );
}

function padDateTimePart(value, length = 2) {
  return String(value).padStart(length, '0');
}

export function formatDateTime(value, options = {}) {
  const { fallback = '-', invalid = 'Invalid Date' } = options;
  if (value === undefined || value === null || value === '') {
    return fallback;
  }
  const date = parseDateTime(value);
  if (Number.isNaN(date.getTime())) {
    return invalid;
  }
  return [
    `${padDateTimePart(date.getFullYear(), 4)}-${padDateTimePart(date.getMonth() + 1)}-${padDateTimePart(date.getDate())}`,
    `${padDateTimePart(date.getHours())}:${padDateTimePart(date.getMinutes())}:${padDateTimePart(date.getSeconds())}`,
  ].join(' ');
}

export function formatFileSize(bytes) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function normalizeUiLocale(value, supportedLocales, fallbackLocale) {
  return supportedLocales.includes(value) ? value : fallbackLocale;
}

export function interpolateMessage(template, values) {
  if (!values) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (match, key) =>
    Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : match,
  );
}

export function createTranslator(options) {
  const { locale, messages, fallbackLocale } = options;
  const currentDictionary = messages[locale] || messages[fallbackLocale] || {};
  const fallbackDictionary = messages[fallbackLocale] || {};
  return (key, values) =>
    interpolateMessage(currentDictionary[key] || fallbackDictionary[key] || key, values);
}

export function createBrowserAuthTokenStore(options) {
  const {
    storageKey,
    changeEvent,
    storage = globalThis.window?.localStorage,
    eventTarget = globalThis.window,
  } = options;

  const dispatchChange = () => {
    if (changeEvent && eventTarget) {
      eventTarget.dispatchEvent(new Event(changeEvent));
    }
  };

  return {
    getAuthToken() {
      return storage?.getItem(storageKey) ?? null;
    },
    setAuthToken(token) {
      storage?.setItem(storageKey, token);
      dispatchChange();
    },
    clearAuthToken() {
      storage?.removeItem(storageKey);
      dispatchChange();
    },
  };
}

export async function readApiErrorMessage(response) {
  let message = response.statusText;
  try {
    const body = await response.json();
    if (typeof body?.error === 'string' && body.error.trim()) {
      return body.error;
    }
    if (typeof body?.error?.message === 'string' && body.error.message.trim()) {
      return body.error.message;
    }
    if (typeof body?.detail === 'string' && body.detail.trim()) {
      return body.detail;
    }
    if (typeof body?.message === 'string' && body.message.trim()) {
      return body.message;
    }
  } catch {
    // Keep the HTTP status text for non-JSON error bodies.
  }
  return message;
}

export async function readJsonResponse(response) {
  const text = await response.text();
  if (!text.trim()) {
    return undefined;
  }
  return JSON.parse(text);
}

export function createJsonApiClient(options = {}) {
  const {
    baseUrl = '',
    getAuthToken = () => null,
    onUnauthorized,
    fetchImpl = globalThis.fetch,
    readErrorMessage = readApiErrorMessage,
    createResponseError,
    readSuccessResponse = readJsonResponse,
    overrideContentType = false,
  } = options;

  if (typeof fetchImpl !== 'function') {
    throw new Error('A fetch implementation is required');
  }

  return async function request(path, init = {}) {
    const headers = new Headers(init.headers);
    if (overrideContentType || !headers.has('Content-Type')) {
      headers.set('Content-Type', 'application/json');
    }
    const token = getAuthToken();
    if (token && !headers.has('Authorization')) {
      headers.set('Authorization', `Bearer ${token}`);
    }

    const response = await fetchImpl(buildApiUrl(baseUrl, path), {
      ...init,
      headers,
    });
    if (!response.ok) {
      const error = createResponseError
        ? await createResponseError(response)
        : new Error(await readErrorMessage(response));
      if (response.status === 401) {
        onUnauthorized?.();
      }
      throw error;
    }
    if (response.status === 204) {
      return undefined;
    }
    return readSuccessResponse(response);
  };
}
