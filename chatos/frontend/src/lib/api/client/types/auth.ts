// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface RegisterPayload {
  username: string;
  email?: string;
  password: string;
  invite_code?: string;
  verification_code?: string;
}

export interface SendRegisterCodePayload {
  email: string;
  invite_code: string;
}

export interface SendRegisterCodeResponse {
  ok?: boolean;
  expires_in_seconds?: number;
  resend_after_seconds?: number;
}

export interface LocalConnectorTicketResponse {
  ticket?: string;
  expires_in_seconds?: number;
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

export interface TaskRunnerAgentAccountResponse {
  id: string;
  username: string;
  display_name?: string | null;
  owner_user_id?: string;
  owner_username?: string;
  enabled?: boolean;
}

export interface WebSocketTicketResponse {
  ticket?: string;
  expires_in?: number;
  expires_at?: string;
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
