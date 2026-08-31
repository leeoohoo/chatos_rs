// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  clearAuthToken as clearSharedAuthToken,
  getAuthToken as getSharedAuthToken,
} from '../../../shared/auth/tokenStore';
import { ADMIN_SERVICE_BASES, buildAdminServiceUrl } from '../../../shared/api/servicePaths';

export type UserRole = 'super_admin' | 'admin' | 'user' | string;

export interface UserSummaryRecord {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_login_at?: string | null;
  agent_count: number;
}

export function buildUserServiceApiUrl(path: string): string {
  return buildAdminServiceUrl(ADMIN_SERVICE_BASES.userService, path);
}

export function getAuthToken(): string | null {
  return getSharedAuthToken();
}

export function clearAuthToken(): void {
  clearSharedAuthToken();
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const token = getAuthToken();
  if (token && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${token}`);
  }

  const response = await fetch(buildUserServiceApiUrl(path), {
    ...init,
    headers,
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      const data = (await response.json()) as { error?: string };
      if (data.error) {
        message = data.error;
      }
    } catch {
      // noop
    }
    if (response.status === 401) {
      clearAuthToken();
    }
    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export const userServiceApi = {
  listUsers: () => request<UserSummaryRecord[]>('/api/users'),
};
