import { createWithEqualityFn } from 'zustand/traditional';
import { persist } from 'zustand/middleware';
import { apiClient } from '@/lib/api/client';

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

interface AuthState {
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
  set: (partial: Partial<AuthState>) => void,
) {
  const record = (response && typeof response === 'object') ? response as Record<string, unknown> : null;
  const token = record?.access_token;
  const user = normalizeAuthUser(record?.user);
  if (typeof token !== 'string' || !token.trim() || !user?.id) {
    throw new Error('认证失败：返回数据不完整');
  }
  apiClient.setAccessToken(token);
  set({
    accessToken: token,
    user,
    initialized: true,
    loading: false,
    error: null,
  });
}

let tokenRefreshListenerRegistered = false;

export const useAuthStore = createWithEqualityFn<AuthState>()(
  persist(
    (set, get) => {
      if (!tokenRefreshListenerRegistered) {
        apiClient.onAccessTokenRefresh((token) => {
          const currentToken = get().accessToken;
          if (!currentToken || currentToken === token) {
            return;
          }
          set({ accessToken: token });
        });
        tokenRefreshListenerRegistered = true;
      }

      return {
        accessToken: null,
        user: null,
        initialized: false,
        loading: false,
        error: null,

        bootstrap: async () => {
          if (get().initialized) {
            return;
          }
          const token = get().accessToken;
          if (!token) {
            apiClient.setAccessToken(null);
            set({ initialized: true, user: null, loading: false, error: null });
            return;
          }

          apiClient.setAccessToken(token);
          set({ loading: true, error: null });
          try {
            const resp = await apiClient.getMe();
            const user = normalizeAuthUser(resp?.user);
            if (!user?.id) {
              throw new Error('登录状态已失效');
            }
            set({ user, initialized: true, loading: false, error: null });
          } catch (error) {
            apiClient.setAccessToken(null);
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
          set({ loading: true, error: null });
          try {
            const resp = await apiClient.login({ username, password });
            applyAuthSuccess(resp, set);
          } catch (error) {
            set({ loading: false, error: extractErrorMessage(error) });
            throw error;
          }
        },

        register: async (username: string, password: string) => {
          set({ loading: true, error: null });
          try {
            const resp = await apiClient.register({ username, password });
            applyAuthSuccess(resp, set);
          } catch (error) {
            set({ loading: false, error: extractErrorMessage(error) });
            throw error;
          }
        },

        logout: () => {
          apiClient.setAccessToken(null);
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
    },
    {
      name: 'chat-auth-store',
      partialize: (state) => ({
        accessToken: state.accessToken,
        user: state.user,
      }),
    }
  )
);
