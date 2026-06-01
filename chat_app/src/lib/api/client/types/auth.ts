export interface RegisterPayload {
  username: string;
  password: string;
}

export interface AuthResponse {
  token?: string;
  access_token?: string;
  user?: {
    id?: string;
    username?: string;
    role?: string;
  } | null;
  username?: string;
  role?: string;
}

export interface MeResponse {
  user?: {
    id?: string;
    username?: string;
    role?: string;
  } | null;
  id?: string;
  username?: string;
  role?: string;
}

export interface UserSettingsResponse {
  user_id?: string;
  settings?: Record<string, unknown>;
  effective?: Record<string, unknown>;
}

export interface UserSettingsUpdatePayload {
  user_id: string;
  settings: Record<string, unknown>;
}

export interface StopChatResponse {
  success?: boolean;
  message?: string;
  conversation_id?: string;
  turn_id?: string | null;
}
