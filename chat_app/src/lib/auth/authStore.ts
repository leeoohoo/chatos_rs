import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { apiClient } from '@/lib/api/client';

export interface AuthUser {
  id: string;
  email: string;
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
  login: (email: string, password: string) => Promise<void>;
  register: (email: string, password: string, displayName?: string) => Promise<void>;
  logout: () => void;
  clearError: () => void;
}

function extractErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return '请求失败，请稍后重试';
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
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
          const user = resp?.user || null;
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

      login: async (email: string, password: string) => {
        set({ loading: true, error: null });
        try {
          const resp = await apiClient.login({ email, password });
          const token = resp?.access_token as string | undefined;
          const user = resp?.user as AuthUser | undefined;
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

      register: async (email: string, password: string, displayName?: string) => {
        set({ loading: true, error: null });
        try {
          const resp = await apiClient.register({
            email,
            password,
            display_name: displayName || undefined,
          });
          const token = resp?.access_token as string | undefined;
          const user = resp?.user as AuthUser | undefined;
          if (!token || !user?.id) {
            throw new Error('注册失败：返回数据不完整');
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
    }),
    {
      name: 'chat-auth-store',
      partialize: (state) => ({
        accessToken: state.accessToken,
        user: state.user,
      }),
    }
  )
);
