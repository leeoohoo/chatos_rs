export interface EngineModelProfile {
  id: string;
  owner_user_id?: string | null;
  owner_username?: string | null;
  name: string;
  provider: string;
  model: string;
  base_url?: string | null;
  api_key?: string | null;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  temperature?: number | null;
  thinking_level?: string | null;
  is_default: boolean;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface UpsertEngineModelProfilePayload {
  name: string;
  provider: string;
  model: string;
  base_url?: string | null;
  api_key?: string | null;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
  temperature?: number | null;
  thinking_level?: string | null;
  is_default?: boolean;
  enabled?: boolean;
}
