export interface EngineSource {
  id: string;
  tenant_id?: string | null;
  source_id: string;
  source_type: string;
  name: string;
  description?: string | null;
  config?: Record<string, unknown> | null;
  status: string;
  sdk_enabled: boolean;
  secret_key_hint?: string | null;
  key_last_rotated_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface UpsertEngineSourcePayload {
  tenant_id?: string | null;
  source_type: string;
  name: string;
  description?: string | null;
  config?: Record<string, unknown> | null;
  sdk_enabled?: boolean;
  status?: string;
}

export interface RotateSourceSecretResponse {
  source: EngineSource;
  secret_key: string;
}
