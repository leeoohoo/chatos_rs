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
  logout: () => void;
  clearError: () => void;
}

function extractErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return '请求失败，请稍后重试';
}

function normalizeAuthUser(input: any): AuthUser | null {
  if (!input || typeof input !== 'object') {
    return null;
  }
  const id =
    String(input.id || input.user_id || input.username || input.email || '').trim();
  if (!id) {
    return null;
  }
  return {
    ...input,
    id,
    username: String(input.username || input.user_id || id).trim() || id,
    email:
      typeof input.email === 'string' && input.email.trim()
        ? input.email.trim()
        : String(input.username || input.user_id || id),
  } as AuthUser;
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
            const token = resp?.access_token as string | undefined;
            const user = normalizeAuthUser(resp?.user);
            if (!token || !user?.id) {
              throw new Error('登录失败：返回数据不完整');
            }
            apiClient.setAccessToken(token);
            set({
              accessToken: token,
              user,
              initialized: true,
              loading: false,
              error: null,
            });
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
