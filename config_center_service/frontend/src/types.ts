export type ConfigValue = string | number | boolean | null | ConfigValue[] | {
  [key: string]: ConfigValue;
};

export interface CurrentUser {
  user_id: string;
  username: string;
  display_name: string;
  role: string;
}
export interface LoginResponse {
  token: string;
  user: CurrentUser;
}

export interface ConfigDefinition {
  id: string;
  key: string;
  display_name: string;
  description: string;
  category: string;
  scope: string;
  service_name?: string | null;
  value_type: string;
  default_value: ConfigValue;
  nullable: boolean;
  min?: number | null;
  max?: number | null;
  enum_options: string[];
  sensitivity: string;
  reload_mode: string;
  criticality: string;
  env_aliases: string[];
  owner_team: string;
  ui_order: number;
  deprecated: boolean;
}

export interface EffectiveConfig {
  environment: string;
  revision: number;
  release_id?: string | null;
  values: Record<string, ConfigValue>;
}

export interface ConfigDraft {
  id: string;
  environment: string;
  base_revision: number;
  changes: Record<string, ConfigValue>;
  validation_status: string;
  validation_errors: string[];
  updated_by: string;
  created_at: string;
  updated_at: string;
}

export interface DraftResponse {
  environment: string;
  active_revision: number;
  draft?: ConfigDraft | null;
}

export interface ConfigRelease {
  id: string;
  environment: string;
  revision: number;
  status: string;
  changed_keys: string[];
  publish_message: string;
  created_by: string;
  created_at: string;
  published_at?: string | null;
  error?: string | null;
}

export interface AuditEvent {
  id: string;
  environment?: string | null;
  action: string;
  actor_display_name: string;
  changed_keys: string[];
  release_id?: string | null;
  created_at: string;
}

export interface ServiceInstance {
  id: string;
  environment: string;
  service_name: string;
  service_id: string;
  running_version?: string | null;
  effective_revision: number;
  effective_checksum: string;
  stale: boolean;
  pending_restart_keys: string[];
  emergency_override_keys: string[];
  last_error?: string | null;
  last_seen_at: string;
}
