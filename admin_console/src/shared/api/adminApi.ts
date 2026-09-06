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

export const adminApi = {
  login: (payload: LoginPayload) =>
    request<{ token: string; user: AdminUser }>('/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  me: () => request<{ user: AdminUser }>('/auth/me'),
  logout: () => request<void>('/auth/logout', { method: 'POST' }),
};
