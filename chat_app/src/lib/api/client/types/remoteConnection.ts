export interface RemoteConnectionResponse {
  id: string;
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  authType?: 'private_key' | 'private_key_cert' | 'password';
  password?: string | null;
  private_key_path?: string | null;
  privateKeyPath?: string | null;
  certificate_path?: string | null;
  certificatePath?: string | null;
  default_remote_path?: string | null;
  defaultRemotePath?: string | null;
  host_key_policy?: 'strict' | 'accept_new';
  hostKeyPolicy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jumpEnabled?: boolean;
  jump_connection_id?: string | null;
  jumpConnectionId?: string | null;
  jump_host?: string | null;
  jumpHost?: string | null;
  jump_port?: number | null;
  jumpPort?: number | null;
  jump_username?: string | null;
  jumpUsername?: string | null;
  jump_private_key_path?: string | null;
  jumpPrivateKeyPath?: string | null;
  jump_certificate_path?: string | null;
  jumpCertificatePath?: string | null;
  jump_password?: string | null;
  jumpPassword?: string | null;
  user_id?: string | null;
  userId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  last_active_at?: string;
  lastActiveAt?: string;
}

export interface RemoteConnectionTestResponse {
  success?: boolean;
  status?: string;
  message?: string;
  error?: string;
  challenge_prompt?: string;
  challengePrompt?: string;
}

export interface RemoteConnectionDraftPayload {
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

export interface RemoteConnectionUpdatePayload {
  name?: string;
  host?: string;
  port?: number;
  username?: string;
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
}
