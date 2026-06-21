export type UserRole = 'super_admin' | 'user';
export type PrincipalType = 'human_user' | 'agent_account';

export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
  principal_type: PrincipalType;
}

export interface LoginPayload {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUser;
}

export interface CurrentUserResponse {
  user: AuthUser;
}

const AUTH_TOKEN_STORAGE_KEY = 'user_service_auth_token';
const AUTH_CHANGED_EVENT = 'user-service-auth-changed';

function normalizeApiBaseUrl(value: string): string {
  return value.replace(/\/+$/, '');
}

function resolveDefaultBaseUrl(): string {
  const host =
    typeof window !== 'undefined' && window.location.hostname
      ? window.location.hostname
      : '127.0.0.1';
  const port = import.meta.env.VITE_USER_SERVICE_PORT ?? '39190';
  return `http://${host}:${port}`;
}

const USER_SERVICE_API_BASE = normalizeApiBaseUrl(
  (import.meta.env.VITE_USER_SERVICE_API_BASE || '').trim() || resolveDefaultBaseUrl(),
);

export function buildUserServiceApiUrl(path: string): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  return `${USER_SERVICE_API_BASE}${normalizedPath}`;
}

export function getAuthToken(): string | null {
  if (typeof window === 'undefined') {
    return null;
  }
  return window.localStorage.getItem(AUTH_TOKEN_STORAGE_KEY);
}

export function setAuthToken(token: string): void {
  if (typeof window === 'undefined') {
    return;
  }
  window.localStorage.setItem(AUTH_TOKEN_STORAGE_KEY, token);
  window.dispatchEvent(new Event(AUTH_CHANGED_EVENT));
}

export function clearAuthToken(): void {
  if (typeof window === 'undefined') {
    return;
  }
  window.localStorage.removeItem(AUTH_TOKEN_STORAGE_KEY);
  window.dispatchEvent(new Event(AUTH_CHANGED_EVENT));
}

export function hasOperatorConsoleAccess(): boolean {
  return Boolean((import.meta.env.VITE_MEMORY_ENGINE_OPERATOR_TOKEN || '').trim());
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
  login: (payload: LoginPayload) =>
    request<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  currentUser: () => request<CurrentUserResponse>('/api/auth/me'),
  logout: () =>
    request<void>('/api/auth/logout', {
      method: 'POST',
    }),
};
