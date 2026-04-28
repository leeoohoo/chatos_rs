export type ApiRequestFn = <T>(endpoint: string, options?: RequestInit) => Promise<T>;

export interface SessionPaging {
  limit?: number;
  offset?: number;
  includeArchived?: boolean;
  includeArchiving?: boolean;
}

export interface ContactPaging {
  limit?: number;
  offset?: number;
}

export interface RemoteConnectionPayload {
  name?: string;
  host: string;
  port?: number;
  username: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jump_connection_id?: string;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_certificate_path?: string;
  jump_password?: string;
  user_id?: string;
}

export const buildRemoteVerificationHeaders = (
  verificationCode?: string,
): Record<string, string> | undefined => {
  const trimmed = verificationCode?.trim();
  return trimmed ? { 'x-remote-verification-code': trimmed } : undefined;
};
