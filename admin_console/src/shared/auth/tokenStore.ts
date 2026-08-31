import { createBrowserAuthTokenStore } from '@chatos/frontend-runtime';

export const ADMIN_AUTH_TOKEN_KEY = 'chatos_admin_auth_token';
export const ADMIN_AUTH_CHANGED_EVENT = 'chatos-admin-auth-changed';

const LEGACY_TOKEN_KEYS = [
  'user_service_auth_token',
  'project_service_auth_token',
  'project_management_service_auth_token',
  'task_runner_auth_token',
  'task_runner_service_auth_token',
  'plugin_management_auth_token',
  'plugin_management_service_auth_token',
  'memory_engine_auth_token',
  'configuration_center_auth_token',
  'chatos.configuration-center.token',
] as const;

const store = createBrowserAuthTokenStore({
  storageKey: ADMIN_AUTH_TOKEN_KEY,
  changeEvent: ADMIN_AUTH_CHANGED_EVENT,
});

export function migrateLegacyAuthToken(): string | null {
  const existing = store.getAuthToken();
  if (existing) {
    return existing;
  }
  for (const key of LEGACY_TOKEN_KEYS) {
    const token = localStorage.getItem(key)?.trim();
    if (token) {
      store.setAuthToken(token);
      return token;
    }
  }
  return null;
}

export const getAuthToken = () => store.getAuthToken();
export const setAuthToken = (token: string) => store.setAuthToken(token);
export const clearAuthToken = () => {
  store.clearAuthToken();
  for (const key of LEGACY_TOKEN_KEYS) {
    localStorage.removeItem(key);
  }
};
