// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type UserRole = 'admin' | 'agent';

export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
}

export interface LoginPayload {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUser;
}

export interface CurrentUserResponse {
  user: AuthUser;
}

export interface UserSummaryRecord {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_login_at?: string | null;
  principal_type?: 'human_user' | 'agent_account' | string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  agent_count?: number | null;
}

export interface CreateUserPayload {
  username: string;
  display_name?: string;
  password: string;
  role?: UserRole;
  enabled?: boolean;
}

export interface UpdateUserPayload {
  display_name?: string;
  password?: string;
  role?: UserRole;
  enabled?: boolean;
}
