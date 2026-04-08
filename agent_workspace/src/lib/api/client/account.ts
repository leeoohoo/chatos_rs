import type {
  AuthResponse,
  MeResponse,
  RegisterPayload,
  UserSettingsResponse,
  UserSettingsUpdatePayload,
} from './types';
import type { ApiRequestFn } from './workspace';

export const register = (
  request: ApiRequestFn,
  data: RegisterPayload,
): Promise<AuthResponse> => {
  return request<AuthResponse>('/auth/register', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const login = (
  request: ApiRequestFn,
  data: RegisterPayload,
): Promise<AuthResponse> => {
  return request<AuthResponse>('/auth/login', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getMe = (request: ApiRequestFn): Promise<MeResponse> => {
  return request<MeResponse>('/auth/me');
};

export const getUserSettings = (request: ApiRequestFn, userId?: string): Promise<UserSettingsResponse> => {
  const qs = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
  return request<UserSettingsResponse>(`/user-settings${qs}`);
};

export const updateUserSettings = (
  request: ApiRequestFn,
  userId: string,
  settings: Record<string, unknown>,
): Promise<UserSettingsResponse> => {
  const payload: UserSettingsUpdatePayload = { user_id: userId, settings };
  return request<UserSettingsResponse>('/user-settings', {
    method: 'PUT',
    body: JSON.stringify(payload),
  });
};
