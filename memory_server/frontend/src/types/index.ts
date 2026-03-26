export interface Session {
  id: string;
  user_id: string;
  project_id?: string | null;
  project_name?: string | null;
  title?: string | null;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface UserItem {
  username: string;
  role: string;
  created_at: string;
  updated_at: string;
}

export interface Message {
  id: string;
  session_id: string;
  role: string;
  content: string;
  summary_status: string;
  created_at: string;
}

export interface SessionSummary {
  id: string;
  session_id: string;
  summary_text: string;
  summary_model: string;
  trigger_type: string;
  source_message_count: number;
  source_estimated_tokens: number;
  status: string;
  level: number;
  rollup_summary_id?: string | null;
  agent_memory_summarized?: number;
  agent_memory_summarized_at?: string | null;
  created_at: string;
  updated_at?: string;
}

export interface AiModelConfig {
  id: string;
  user_id: string;
  name: string;
  provider: string;
  model: string;
  base_url?: string | null;
  api_key?: string | null;
  supports_images: number;
  supports_reasoning: number;
  supports_responses: number;
  temperature?: number | null;
  thinking_level?: string | null;
  enabled: number;
  created_at: string;
  updated_at: string;
}

export interface SummaryJobConfig {
  user_id: string;
  enabled: number;
  summary_model_config_id?: string | null;
  summary_prompt?: string | null;
  token_limit: number;
  round_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
  max_sessions_per_tick: number;
}

export interface RollupJobConfig {
  user_id: string;
  enabled: number;
  summary_model_config_id?: string | null;
  summary_prompt?: string | null;
  token_limit: number;
  round_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
  keep_raw_level0_count: number;
  max_level: number;
  max_sessions_per_tick: number;
}

export interface JobRun {
  id: string;
  job_type: string;
  session_id?: string | null;
  status: string;
  trigger_type?: string | null;
  input_count: number;
  output_count: number;
  error_message?: string | null;
  started_at: string;
  finished_at?: string | null;
}

export interface SummaryLevelItem {
  level: number;
  total: number;
  pending: number;
  summarized: number;
}

export interface SummaryGraphNode {
  id: string;
  level: number;
  status: string;
  rollup_summary_id?: string | null;
  created_at: string;
  summary_excerpt: string;
}

export interface SummaryGraphEdge {
  from: string;
  to: string;
}

export interface MemoryAgentSkill {
  id: string;
  name: string;
  content: string;
}

export interface MemoryAgent {
  id: string;
  user_id: string;
  name: string;
  description?: string | null;
  category?: string | null;
  role_definition: string;
  plugin_sources: string[];
  skills: MemoryAgentSkill[];
  skill_ids: string[];
  default_skill_ids: string[];
  mcp_policy?: Record<string, unknown> | null;
  project_policy?: Record<string, unknown> | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface MemorySkillPlugin {
  id: string;
  user_id: string;
  source: string;
  name: string;
  category?: string | null;
  description?: string | null;
  version?: string | null;
  repository?: string | null;
  branch?: string | null;
  cache_path?: string | null;
  content?: string | null;
  commands?: MemorySkillPluginCommand[];
  command_count?: number;
  installed: boolean;
  discoverable_skills: number;
  installed_skill_count: number;
  updated_at: string;
}

export interface MemorySkillPluginCommand {
  name: string;
  source_path: string;
  content: string;
}

export interface MemorySkill {
  id: string;
  user_id: string;
  plugin_source: string;
  name: string;
  description?: string | null;
  content: string;
  source_path: string;
  version?: string | null;
  updated_at: string;
}

export interface MemoryContact {
  id: string;
  user_id: string;
  agent_id: string;
  agent_name_snapshot?: string | null;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface ProjectMemory {
  id: string;
  user_id: string;
  contact_id: string;
  agent_id: string;
  project_id: string;
  memory_text: string;
  memory_version: number;
  recall_summarized: number;
  recall_summarized_at?: string | null;
  last_source_at?: string | null;
  updated_at: string;
}

export interface ContactProject {
  project_id: string;
  project_name?: string | null;
  project_root?: string | null;
  status?: string;
  is_virtual?: number;
  has_memory?: boolean;
  memory_version?: number;
  recall_summarized?: number;
  last_source_at?: string | null;
  updated_at: string;
}

export interface MemoryProject {
  id: string;
  user_id: string;
  project_id: string;
  name: string;
  root_path?: string | null;
  description?: string | null;
  status?: string | null;
  is_virtual?: number;
  created_at: string;
  updated_at: string;
}

export interface AgentRecall {
  id: string;
  user_id: string;
  agent_id: string;
  recall_key: string;
  recall_text: string;
  level: number;
  rolled_up?: number;
  rollup_recall_key?: string | null;
  rolled_up_at?: string | null;
  confidence?: number | null;
  last_seen_at?: string | null;
  updated_at: string;
}

export interface AgentMemoryJobConfig {
  user_id: string;
  enabled: number;
  summary_model_config_id?: string | null;
  summary_prompt?: string | null;
  token_limit: number;
  round_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
  keep_raw_level0_count: number;
  max_level: number;
  max_agents_per_tick: number;
}
