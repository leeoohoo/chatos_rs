import type { ApiRequestFn } from './workspace';

export const register = (
  request: ApiRequestFn,
  data: {
    email: string;
    password: string;
    display_name?: string;
  }
): Promise<any> => {
  return request<any>('/auth/register', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const login = (
  request: ApiRequestFn,
  data: { email: string; password: string }
): Promise<any> => {
  return request<any>('/auth/login', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getMe = (request: ApiRequestFn): Promise<any> => {
  return request<any>('/auth/me');
};

export const getUserSettings = (request: ApiRequestFn, userId?: string): Promise<any> => {
  const qs = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
  return request<any>(`/user-settings${qs}`);
};

export const updateUserSettings = (
  request: ApiRequestFn,
  userId: string,
  settings: Record<string, any>
): Promise<any> => {
  return request<any>(`/user-settings`, {
    method: 'PUT',
    body: JSON.stringify({ user_id: userId, settings }),
  });
};
