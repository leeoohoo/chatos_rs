// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { persist } from 'zustand/middleware';
import { createWithEqualityFn, useStoreWithEqualityFn } from 'zustand/traditional';
import {
  clearAnonymousChatStoreState,
  clearLegacyChatStoreState,
} from '@/lib/store/persistence';
import ApiClient, { apiClient as globalApiClient } from '@/lib/api/client';
import { ApiRequestError } from '@/lib/api/client/shared';
import { useApiClientContext } from '@/lib/api/ApiClientContext';

export interface AuthUser {
  id: string;
  username?: string;
  email?: string | null;
  role?: string;
  display_name?: string | null;
  status?: string;
  created_at?: string;
  updated_at?: string;
  last_login_at?: string | null;
}

export interface AuthState {
  accessToken: string | null;
  user: AuthUser | null;
  initialized: boolean;
  loading: boolean;
  error: string | null;
  bootstrap: () => Promise<void>;
  login: (username: string, password: string) => Promise<void>;
  register: (email: string, password: string, inviteCode: string, verificationCode: string) => Promise<void>;
  sendRegisterEmailCode: (email: string, inviteCode: string) => Promise<void>;
  logout: () => void;
  clearError: () => void;
}

function extractErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return '请求失败，请稍后重试';
}

const authFailureInvalidatesSession = (error: unknown): boolean => (
  error instanceof ApiRequestError && (error.status === 401 || error.status === 403)
);

const readStringField = (record: Record<string, unknown>, key: string): string => {
  const value = record[key];
  return typeof value === 'string' ? value : '';
};

function normalizeAuthUser(input: unknown): AuthUser | null {
  if (!input || typeof input !== 'object') {
    return null;
  }
  const record = input as Record<string, unknown>;
  const id =
    String(
      readStringField(record, 'id')
      || readStringField(record, 'user_id')
      || readStringField(record, 'username')
      || readStringField(record, 'email')
      || '',
    ).trim();
  if (!id) {
    return null;
  }
  const username = String(
    readStringField(record, 'username') || readStringField(record, 'user_id') || id,
  ).trim() || id;
  const rawEmail = readStringField(record, 'email');
  return {
    ...(input as AuthUser),
    id,
    username,
    email:
      rawEmail.trim()
        ? rawEmail.trim()
        : String(readStringField(record, 'username') || readStringField(record, 'user_id') || id),
  };
}

interface ParsedAuthSuccess {
  token: string;
  user: AuthUser;
}

class LocalConnectorDesktopSyncError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'LocalConnectorDesktopSyncError';
  }
}

function parseAuthSuccess(
  response: unknown,
  client: ApiClient,
): ParsedAuthSuccess {
  const record = (response && typeof response === 'object') ? response as Record<string, unknown> : null;
  const token = record?.access_token;
  const user = normalizeAuthUser(record?.user);
  if (typeof token !== 'string' || !token.trim() || !user?.id) {
    throw new Error('认证失败：返回数据不完整');
  }
  client.setAccessToken(token);
  return { token, user };
}

function commitAuthSuccess(
  auth: ParsedAuthSuccess,
  set: (partial: Partial<AuthState>) => void,
) {
  set({
    accessToken: auth.token,
    user: auth.user,
    initialized: true,
    loading: false,
    error: null,
  });
}

function isLocalConnectorDesktopHost(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }
  try {
    const params = new URLSearchParams(window.location.search);
    if (params.get('desktop') === 'local-connector') {
      window.sessionStorage.setItem('chatos-local-connector-desktop', '1');
      return true;
    }
    return window.sessionStorage.getItem('chatos-local-connector-desktop') === '1';
  } catch {
    return false;
  }
}

async function syncLocalConnectorDesktop(client: ApiClient): Promise<void> {
  if (!isLocalConnectorDesktopHost()) {
    return;
  }
  try {
    const response = await client.issueLocalConnectorTicket();
    const ticket = String(response?.ticket || '').trim();
    if (!ticket) {
      throw new Error('授权票据为空');
    }
    const authenticate = window.chatosLocalRuntime?.authenticateDesktopTicket;
    if (typeof authenticate === 'function') {
      await authenticate(ticket);
      return;
    }
    window.location.href = `chatos-local-connector://auth?ticket=${encodeURIComponent(ticket)}`;
  } catch (error) {
    throw new LocalConnectorDesktopSyncError(
      `Local Connector 登录同步失败：${extractErrorMessage(error)}`,
    );
  }
}

const AUTH_STORE_KEY = 'chat-auth-store';

const sanitizeStorageSegment = (value: string): string => (
  value
    .trim()
    .replace(/[^a-zA-Z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .toLowerCase()
);

const resolveAuthStoreStorageKey = (
  client: ApiClient,
  explicitKey?: string,
): string => {
  if (explicitKey?.trim()) {
    return explicitKey.trim();
  }
  if (client === globalApiClient) {
    return AUTH_STORE_KEY;
  }
  const sanitizedBaseUrl = sanitizeStorageSegment(client.getBaseUrl());
  return sanitizedBaseUrl
    ? `${AUTH_STORE_KEY}:${sanitizedBaseUrl}`
    : AUTH_STORE_KEY;
};

const buildAuthStateStore = createWithEqualityFn<AuthState>();

export const createAuthStore = (
  client: ApiClient = globalApiClient,
  options?: { storageKey?: string },
)=> buildAuthStateStore(
  persist(
    (set, get) => {
      const tokenRefreshUnsubscribe = client.onAccessTokenRefresh((token) => {
        const currentToken = get().accessToken;
        if (!currentToken || currentToken === token) {
          return;
        }
        set({ accessToken: token });
      });

      const getClient = (): ApiClient => {
        client.setAccessToken(get().accessToken);
        return client;
      };

      let bootstrapInFlight: Promise<void> | null = null;

      const runBootstrap = async (): Promise<void> => {
        const runtimeClient = getClient();
        const token = get().accessToken;
        if (!token) {
          runtimeClient.setAccessToken(null);
          set({ initialized: true, user: null, loading: false, error: null });
          return;
        }

        runtimeClient.setAccessToken(token);
        set({ loading: true, error: null });
        try {
          const resp = await runtimeClient.getMe();
          const user = normalizeAuthUser(resp?.user);
          if (!user?.id) {
            runtimeClient.setAccessToken(null);
            set({
              accessToken: null,
              user: null,
              initialized: true,
              loading: false,
              error: null,
            });
            return;
          }
          await syncLocalConnectorDesktop(runtimeClient);
          set({ user, initialized: true, loading: false, error: null });
        } catch (error) {
          if (authFailureInvalidatesSession(error)) {
            runtimeClient.setAccessToken(null);
            set({
              accessToken: null,
              user: null,
              initialized: true,
              loading: false,
              error: null,
            });
            return;
          }
          if (error instanceof LocalConnectorDesktopSyncError) {
            runtimeClient.setAccessToken(null);
            set({
              accessToken: null,
              user: null,
              initialized: true,
              loading: false,
              error: error.message,
            });
            return;
          }
          console.warn('Unable to validate the persisted login during startup; keeping the local session.', error);
          set({
            initialized: true,
            loading: false,
            error: null,
          });
        }
      };

      const storeState: AuthState = {
        accessToken: null,
        user: null,
        initialized: false,
        loading: false,
        error: null,

        bootstrap: async () => {
          if (get().initialized) {
            return;
          }
          if (!bootstrapInFlight) {
            bootstrapInFlight = runBootstrap().finally(() => {
              bootstrapInFlight = null;
            });
          }
          await bootstrapInFlight;
        },

        login: async (username: string, password: string) => {
          const runtimeClient = getClient();
          set({ loading: true, error: null });
          try {
            const resp = await runtimeClient.login({ username, password });
            const auth = parseAuthSuccess(resp, runtimeClient);
            await syncLocalConnectorDesktop(runtimeClient);
            commitAuthSuccess(auth, set);
          } catch (error) {
            runtimeClient.setAccessToken(null);
            set({
              accessToken: null,
              user: null,
              initialized: true,
              loading: false,
              error: extractErrorMessage(error),
            });
            throw error;
          }
        },

        sendRegisterEmailCode: async (email: string, inviteCode: string) => {
          const runtimeClient = getClient();
          set({ loading: true, error: null });
          try {
            await runtimeClient.sendRegisterEmailCode({ email, invite_code: inviteCode });
            set({ loading: false, error: null });
          } catch (error) {
            set({ loading: false, error: extractErrorMessage(error) });
            throw error;
          }
        },

        register: async (email: string, password: string, inviteCode: string, verificationCode: string) => {
          const runtimeClient = getClient();
          set({ loading: true, error: null });
          try {
            const resp = await runtimeClient.register({
              username: email,
              email,
              password,
              invite_code: inviteCode,
              verification_code: verificationCode,
            });
            const auth = parseAuthSuccess(resp, runtimeClient);
            await syncLocalConnectorDesktop(runtimeClient);
            commitAuthSuccess(auth, set);
          } catch (error) {
            runtimeClient.setAccessToken(null);
            set({
              accessToken: null,
              user: null,
              initialized: true,
              loading: false,
              error: extractErrorMessage(error),
            });
            throw error;
          }
        },

        logout: () => {
          const runtimeClient = getClient();
          runtimeClient.setAccessToken(null);
          clearLegacyChatStoreState();
          clearAnonymousChatStoreState();
          set({
            accessToken: null,
            user: null,
            initialized: true,
            loading: false,
            error: null,
          });
        },

        clearError: () => set({ error: null }),
      };

      void tokenRefreshUnsubscribe;
      return storeState;
    },
    {
      name: resolveAuthStoreStorageKey(client, options?.storageKey),
      partialize: (state) => ({
        accessToken: state.accessToken,
        user: state.user,
      }),
    }
  )
);

const globalAuthStore = createAuthStore();
type AuthStoreHook = ReturnType<typeof createAuthStore>;
const AuthStoreContext = React.createContext<AuthStoreHook | null>(null);
const AuthStorePresenceContext = React.createContext(false);

interface AuthStoreProviderProps {
  children: React.ReactNode;
  customApiClient?: ApiClient;
  storageKey?: string;
}

export const AuthStoreProvider: React.FC<AuthStoreProviderProps> = ({
  children,
  customApiClient,
  storageKey,
}) => {
  const resolvedApiClient = useApiClientContext();
  const effectiveApiClient = customApiClient || resolvedApiClient;
  const store = React.useMemo(
    () => createAuthStore(effectiveApiClient, { storageKey }),
    [effectiveApiClient, storageKey],
  );

  return React.createElement(
    AuthStorePresenceContext.Provider,
    { value: true },
    React.createElement(AuthStoreContext.Provider, { value: store }, children),
  );
};

export const useOptionalAuthStoreContext = (): AuthStoreHook | null => React.useContext(AuthStoreContext);

export const useAuthStoreContext = (): AuthStoreHook => {
  const store = React.useContext(AuthStoreContext);
  if (!store) {
    throw new Error('useAuthStoreContext must be used within an AuthStoreProvider');
  }
  return store;
};

export const useAuthStoreFromContext = (): AuthState => {
  const store = useAuthStoreContext();
  return useStoreWithEqualityFn(store);
};

export const useAuthStoreSelector = <T,>(
  selector: (state: AuthState) => T,
  equalityFn?: (left: T, right: T) => boolean,
): T => {
  const store = useAuthStoreContext();
  return useStoreWithEqualityFn(store, selector, equalityFn);
};

export const useOptionalAuthStoreSelector = <T,>(
  selector: (state: AuthState) => T,
  equalityFn?: (left: T, right: T) => boolean,
): T | null => {
  const hasProvider = React.useContext(AuthStorePresenceContext);
  const contextStore = React.useContext(AuthStoreContext);
  const store = contextStore ?? globalAuthStore;
  const selected = useStoreWithEqualityFn(store, selector, equalityFn);
  return hasProvider ? selected : null;
};

function useAuthStoreWithFallback(): AuthState;
function useAuthStoreWithFallback<T>(
  selector: (state: AuthState) => T,
  equalityFn?: (left: T, right: T) => boolean,
): T;
function useAuthStoreWithFallback<T>(
  selector?: (state: AuthState) => T,
  equalityFn?: (left: T, right: T) => boolean,
) {
  const contextStore = React.useContext(AuthStoreContext);
  const store = contextStore ?? globalAuthStore;
  const effectiveSelector = (selector ?? ((state: AuthState) => state)) as (
    state: AuthState,
  ) => T;
  return useStoreWithEqualityFn(store, effectiveSelector, equalityFn);
}

export const useAuthStore = Object.assign(
  useAuthStoreWithFallback,
  globalAuthStore,
) as AuthStoreHook;
