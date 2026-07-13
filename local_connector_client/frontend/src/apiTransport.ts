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
    const message =
      typeof body?.error === 'string'
        ? body.error
        : typeof body?.message === 'string'
          ? body.message
          : `HTTP ${response.status}`;
    throw new Error(message);
  }
  return body as T;
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
