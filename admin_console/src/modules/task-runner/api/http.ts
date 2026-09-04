// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  buildApiUrl as buildSharedApiUrl,
  createJsonApiClient,
  withQuery as buildQuery,
} from '@chatos/frontend-runtime';
import {
  clearAuthToken as clearSharedAuthToken,
  getAuthToken as getSharedAuthToken,
} from '../../../shared/auth/tokenStore';
import { ADMIN_SERVICE_BASES, stripBackendApiPrefix } from '../../../shared/api/servicePaths';

const API_BASE_URL = ADMIN_SERVICE_BASES.taskRunner;

export function getAuthToken(): string | null {
  return getSharedAuthToken();
}

export function clearAuthToken(): void {
  clearSharedAuthToken();
}

export function buildApiUrl(path: string): string {
  return buildSharedApiUrl(API_BASE_URL, stripBackendApiPrefix(path));
}

export function buildEventSourceUrl(path: string, sseTicket: string): string {
  const url = buildApiUrl(path);
  const separator = url.includes('?') ? '&' : '?';
  return `${url}${separator}sse_ticket=${encodeURIComponent(sseTicket)}`;
}

const rawRequest = createJsonApiClient({
  baseUrl: API_BASE_URL,
  timeoutMs: 30_000,
  getAuthToken,
  onUnauthorized: clearAuthToken,
});

export const request = <T,>(path: string, init?: RequestInit) =>
  rawRequest<T>(stripBackendApiPrefix(path), init);

export function withQuery(path: string, params: Record<string, string | undefined>): string {
  return buildQuery(path, params);
}
