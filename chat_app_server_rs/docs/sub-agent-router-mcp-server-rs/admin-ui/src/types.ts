export interface ModelConfig {
  id?: string;
  name?: string;
  api_key: string;
  base_url: string;
  model: string;
  reasoning_enabled: boolean;
  responses_enabled: boolean;
}

export interface RuntimeConfig {
  aiTimeoutMs?: number | null;
  aiMaxOutputBytes?: number | null;
  aiToolMaxTurns?: number | null;
  aiMaxRetries?: number | null;
  commandTimeoutMs?: number | null;
  commandMaxOutputBytes?: number | null;
}

export interface MarketplaceRecord {
  id: string;
  name: string;
  pluginCount: number;
  active: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface McpServerConfig {
  id: string;
  name: string;
  transport: string;
  command: string;
  args: string[];
  endpointUrl: string;
  headersJson: string;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface JobRecord {
  id: string;
  status: string;
  task: string;
  agentId?: string;
  commandId?: string;
  payloadJson?: string;
  resultJson?: string;
  error?: string;
  createdAt: string;
  updatedAt: string;
  sessionId: string;
  runId: string;
}

export interface JobEvent {
  id: string;
  jobId: string;
  type: string;
  payloadJson?: string;
  createdAt: string;
  sessionId: string;
  runId: string;
}

export interface StatusResponse {
  allow_prefixes: string[];
  marketplaces: MarketplaceRecord[];
  active_marketplaces: string[];
  marketplace_path: string;
  plugins_root: string;
  plugins_source_root: string;
  registry_path: string;
  db_path: string;
  model_config: ModelConfig;
  model_configs: ModelConfig[];
  active_model_id: string;
  runtime_config: RuntimeConfig;
}

export interface MarketplaceEntrySummary {
  id: string;
  title: string;
  path: string;
  exists: boolean;
}

export interface PluginSummary {
  name: string;
  source: string;
  category: string;
  description: string;
  version: string;
  repository: string;
  homepage: string;
  exists: boolean;
  counts: {
    agents: { total: number; available: number };
    skills: { total: number; available: number };
    commands: { total: number; available: number };
  };
  agents: MarketplaceEntrySummary[];
  skills: MarketplaceEntrySummary[];
  commands: MarketplaceEntrySummary[];
}

export interface MarketplaceSummary {
  plugins: PluginSummary[];
  counts: {
    agents: { total: number; available: number };
    skills: { total: number; available: number };
    commands: { total: number; available: number };
  };
}
