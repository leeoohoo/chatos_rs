// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  createBrowserAuthTokenStore,
  createJsonApiClient,
  normalizeApiBaseUrl,
} from '@chatos/frontend-runtime';

const RAW_API_BASE_URL = (
  import.meta.env.VITE_API_BASE_URL ||
  (import.meta.env.BASE_URL && import.meta.env.BASE_URL !== '/' ? import.meta.env.BASE_URL : '')
).trim();
const API_BASE_URL = normalizeApiBaseUrl(RAW_API_BASE_URL);
const AUTH_TOKEN_STORAGE_KEY = 'user_service_auth_token';
const authTokenStore = createBrowserAuthTokenStore({ storageKey: AUTH_TOKEN_STORAGE_KEY });

export { withQuery } from '@chatos/frontend-runtime';
export type { QueryValue } from '@chatos/frontend-runtime';

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
  return authTokenStore.getAuthToken()?.trim() || null;
}

async function createApiRequestError(response: Response): Promise<ApiRequestError> {
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
  return new ApiRequestError(message || `HTTP ${response.status}`, response.status, code);
}

const jsonRequest = createJsonApiClient({
  baseUrl: API_BASE_URL,
  getAuthToken,
  createResponseError: createApiRequestError,
});

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  return jsonRequest<T>(path, init);
}
