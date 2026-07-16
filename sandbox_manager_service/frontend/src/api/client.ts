// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const RAW_API_BASE_URL = (
  import.meta.env.VITE_API_BASE_URL ||
  (import.meta.env.BASE_URL && import.meta.env.BASE_URL !== '/' ? import.meta.env.BASE_URL : '')
).trim();
const API_BASE_URL = RAW_API_BASE_URL.replace(/\/+$/, '').replace(/\/api$/, '');
const AUTH_TOKEN_STORAGE_KEY = 'user_service_auth_token';

export class ApiRequestError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
  ) {
    super(message);
    this.name = 'ApiRequestError';
  }
}

function getAuthToken(): string | null {
  if (typeof window === 'undefined') {
    return null;
  }
  return window.localStorage.getItem(AUTH_TOKEN_STORAGE_KEY);
}

function buildApiUrl(path: string): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  return API_BASE_URL ? `${API_BASE_URL}${normalizedPath}` : normalizedPath;
}

export type QueryValue = string | number | boolean | null | undefined;

export function withQuery(path: string, params: Record<string, QueryValue>): string {
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

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const authToken = getAuthToken()?.trim();
  if (authToken && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${authToken}`);
  }
  const response = await fetch(buildApiUrl(path), { ...init, headers });
  if (!response.ok) {
    let message = response.statusText;
    let code: string | undefined;
    try {
      const data = (await response.json()) as { error?: { code?: string; message?: string } };
      code = data.error?.code;
      if (data.error?.message) {
        message = data.error.message;
      }
    } catch {
      // keep status text
    }
    throw new ApiRequestError(message || `HTTP ${response.status}`, response.status, code);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  const text = await response.text();
  return text.trim() ? (JSON.parse(text) as T) : (undefined as T);
}
