// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  buildApiUrl as buildSharedApiUrl,
  createBrowserAuthTokenStore,
  createJsonApiClient,
  normalizeApiBaseUrl,
  withQuery as buildQuery,
} from '@chatos/frontend-runtime';

const RAW_API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || '').trim();
const API_BASE_URL = normalizeApiBaseUrl(RAW_API_BASE_URL);
const AUTH_TOKEN_STORAGE_KEY = 'task_runner_service_auth_token';
const authTokenStore = createBrowserAuthTokenStore({
  storageKey: AUTH_TOKEN_STORAGE_KEY,
  changeEvent: 'task-runner-auth-changed',
});

export function getAuthToken(): string | null {
  return authTokenStore.getAuthToken();
}

export function setAuthToken(token: string): void {
  authTokenStore.setAuthToken(token);
}

export function clearAuthToken(): void {
  authTokenStore.clearAuthToken();
}

export function buildApiUrl(path: string): string {
  return buildSharedApiUrl(API_BASE_URL, path);
}

export function buildEventSourceUrl(path: string, sseTicket: string): string {
  const url = buildApiUrl(path);
  const separator = url.includes('?') ? '&' : '?';
  return `${url}${separator}sse_ticket=${encodeURIComponent(sseTicket)}`;
}

export const request = createJsonApiClient({
  baseUrl: API_BASE_URL,
  getAuthToken,
  onUnauthorized: clearAuthToken,
});

export function withQuery(path: string, params: Record<string, string | undefined>): string {
  return buildQuery(path, params);
}
