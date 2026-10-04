export const ADMIN_AUTH_TOKEN_KEY = 'chatos_admin_auth_token';
export const ADMIN_AUTH_CHANGED_EVENT = 'chatos-admin-auth-changed';

const LEGACY_TOKEN_KEYS = [
  'user_service_auth_token',
  'plugin_management_auth_token',
  'plugin_management_service_auth_token',
  'memory_engine_auth_token',
  'configuration_center_auth_token',
  'chatos.configuration-center.token',
] as const;

const PERSISTED_TOKEN_KEYS = [ADMIN_AUTH_TOKEN_KEY, ...LEGACY_TOKEN_KEYS] as const;
let authToken: string | null = null;

function dispatchAuthChanged() {
  globalThis.window?.dispatchEvent(new Event(ADMIN_AUTH_CHANGED_EVENT));
}

function clearPersistedAuthTokens() {
  try {
    for (const key of PERSISTED_TOKEN_KEYS) {
      globalThis.localStorage?.removeItem(key);
    }
  } catch {
    // Storage may be unavailable under hardened browser privacy policies.
  }
}

// Do not leave a previously persisted administrator token readable by scripts.
clearPersistedAuthTokens();

export function migrateLegacyAuthToken(): string | null {
  clearPersistedAuthTokens();
  return authToken;
}

export const getAuthToken = () => authToken;
export const setAuthToken = (token: string) => {
  authToken = token;
  clearPersistedAuthTokens();
  dispatchAuthChanged();
};
export const clearAuthToken = () => {
  authToken = null;
  clearPersistedAuthTokens();
  dispatchAuthChanged();
};
