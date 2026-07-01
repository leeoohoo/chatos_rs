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
  register: (username: string, password: string) => Promise<void>;
  logout: () => void;
  clearError: () => void;
}

function extractErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return '请求失败，请稍后重试';
}

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

function applyAuthSuccess(
  response: unknown,
  client: ApiClient,
  set: (partial: Partial<AuthState>) => void,
) {
  const record = (response && typeof response === 'object') ? response as Record<string, unknown> : null;
  const token = record?.access_token;
  const user = normalizeAuthUser(record?.user);
  if (typeof token !== 'string' || !token.trim() || !user?.id) {
    throw new Error('认证失败：返回数据不完整');
  }
  client.setAccessToken(token);
  set({
    accessToken: token,
    user,
    initialized: true,
    loading: false,
    error: null,
  });
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
              throw new Error('登录状态已失效');
            }
            set({ user, initialized: true, loading: false, error: null });
          } catch (error) {
            runtimeClient.setAccessToken(null);
            set({
              accessToken: null,
              user: null,
              initialized: true,
              loading: false,
              error: null,
            });
          }
        },

        login: async (username: string, password: string) => {
          const runtimeClient = getClient();
          set({ loading: true, error: null });
          try {
            const resp = await runtimeClient.login({ username, password });
            applyAuthSuccess(resp, runtimeClient, set);
          } catch (error) {
            set({ loading: false, error: extractErrorMessage(error) });
            throw error;
          }
        },

        register: async (username: string, password: string) => {
          const runtimeClient = getClient();
          set({ loading: true, error: null });
          try {
            const resp = await runtimeClient.register({ username, password });
            applyAuthSuccess(resp, runtimeClient, set);
          } catch (error) {
            set({ loading: false, error: extractErrorMessage(error) });
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
