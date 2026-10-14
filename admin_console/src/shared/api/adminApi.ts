import { createJsonApiClient } from '@chatos/frontend-runtime';

import { clearAuthToken, getAuthToken } from '../auth/tokenStore';

export type AdminRole = 'super_admin' | 'admin' | 'user' | string;

export interface AdminUser {
  id: string;
  username: string;
  display_name: string;
  role: AdminRole;
  principal_type?: string;
}

export interface LoginPayload {
  username: string;
  password: string;
}

const request = createJsonApiClient({
  baseUrl: '/api/admin/user-service',
  timeoutMs: 30_000,
  getAuthToken,
  onUnauthorized: clearAuthToken,
  readSuccessResponse: (response) => response.json(),
});

export class AdminSessionError extends Error {
  constructor(public readonly status: number, message: string) {
    super(message);
  }
}

const sessionRequest = createJsonApiClient({
  baseUrl: '/api/admin/user-service/auth/admin',
  timeoutMs: 30_000,
  createResponseError: async (response) => {
    const body = await response.json().catch(() => ({}));
    return new AdminSessionError(response.status, body.error || response.statusText);
  },
});

const browserSessionPost = {
  method: 'POST',
  credentials: 'same-origin',
  headers: { 'X-Chatos-Admin-Session': '1' },
} as const;

export const adminApi = {
  login: (payload: LoginPayload) =>
    sessionRequest<{ token: string; user: AdminUser }>('/login', {
      ...browserSessionPost,
      body: JSON.stringify(payload),
    }),
  restoreSession: () => sessionRequest<{ token: string; user: AdminUser }>('/session', browserSessionPost),
  me: () => request<{ user: AdminUser }>('/auth/me'),
  logout: () => sessionRequest<void>('/logout', browserSessionPost),
};
