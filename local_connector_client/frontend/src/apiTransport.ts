// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const API_BASE_URL = normalizeApiBaseUrl(import.meta.env.VITE_LOCAL_CONNECTOR_CORE_URL);

interface ApiTransportResponse {
  ok: boolean;
  status: number;
  body: string;
}

function normalizeApiBaseUrl(value: unknown): string {
  return typeof value === 'string' ? value.trim().replace(/\/+$/, '') : '';
}

function apiEndpoint(endpoint: string): string {
  if (!API_BASE_URL || /^https?:\/\//i.test(endpoint)) {
    return endpoint;
  }
  return `${API_BASE_URL}${endpoint}`;
}

export async function request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
  const headers = new Headers(options.headers || {});
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const response = await sendLocalApiRequest(endpoint, options, headers);
  const text = response.body;
  const body = text ? JSON.parse(text) : null;
  if (!response.ok) {
    throw apiErrorFromBody(body, response.status);
  }
  return body as T;
}

function apiErrorFromBody(body: unknown, status: number): Error {
  const record = isRecord(body) ? body : {};
  const error = record.error;
  const errorRecord = isRecord(error) ? error : {};
  const message =
    typeof error === 'string'
      ? error
      : typeof errorRecord.message === 'string'
        ? errorRecord.message
        : typeof record.message === 'string'
          ? record.message
          : `HTTP ${status}`;
  const code =
    typeof record.code === 'string'
      ? record.code
      : typeof errorRecord.code === 'string'
        ? errorRecord.code
        : undefined;
  const err = new Error(message) as Error & { code?: string; status?: number };
  err.status = status;
  if (code) {
    err.code = code;
  }
  return err;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

async function sendLocalApiRequest(
  endpoint: string,
  options: RequestInit,
  headers: Headers,
): Promise<ApiTransportResponse> {
  const bridge = window.chatosLocalConnector?.apiRequest;
  if (bridge && !/^https?:\/\//i.test(endpoint)) {
    const response = await bridge({
      method: options.method || 'GET',
      endpoint,
      headers: Object.fromEntries(headers.entries()),
      body: normalizeBridgeBody(options.body),
    });
    return {
      ok: response.ok,
      status: response.status,
      body: response.body,
    };
  }

  const response = await fetch(apiEndpoint(endpoint), {
    ...options,
    headers,
  });
  return {
    ok: response.ok,
    status: response.status,
    body: await response.text(),
  };
}

function normalizeBridgeBody(body: BodyInit | null | undefined): string | null {
  if (body === undefined || body === null) {
    return null;
  }
  if (typeof body === 'string') {
    return body;
  }
  throw new Error('Electron desktop API bridge only supports string request bodies');
}
