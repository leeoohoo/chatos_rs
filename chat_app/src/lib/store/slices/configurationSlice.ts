import type {
  AgentConfig,
  AiModelConfig,
  Application,
  ChatConfig,
  McpConfig,
  SystemContext,
} from '../../../types';
import type {
  SystemContextDraftEvaluateResponse,
  SystemContextDraftGenerateResponse,
  SystemContextDraftOptimizeResponse,
  SystemContextModelConfigPayload,
  SystemContextResponse,
} from '../../api/client/types';

export interface ConfigurationSliceState {
  chatConfig: ChatConfig;
  mcpConfigs: McpConfig[];
  aiModelConfigs: AiModelConfig[];
  selectedModelId: string | null;
  agents: AgentConfig[];
  selectedAgentId: string | null;
  systemContexts: SystemContext[];
  activeSystemContext: SystemContext | null;
  applications: Application[];
  selectedApplicationId: string | null;
}

export const configurationInitialState: ConfigurationSliceState = {
  chatConfig: {
    model: 'gpt-4',
    temperature: 0.7,
    systemPrompt: '',
    enableMcp: true,
    reasoningEnabled: false,
  },
  mcpConfigs: [],
  aiModelConfigs: [],
  selectedModelId: null,
  agents: [],
  selectedAgentId: null,
  systemContexts: [],
  activeSystemContext: null,
  applications: [],
  selectedApplicationId: null,
};

export interface ConfigurationSliceActions {
  updateChatConfig: (config: Partial<ChatConfig>) => Promise<void>;
  loadMcpConfigs: (options?: { forceRefresh?: boolean }) => Promise<void>;
  updateMcpConfig: (config: McpConfig) => Promise<McpConfig | null>;
  deleteMcpConfig: (id: string) => Promise<void>;
  loadAiModelConfigs: (options?: { force?: boolean }) => Promise<void>;
  updateAiModelConfig: (
    config: AiModelConfig,
    options?: { clearApiKey?: boolean },
  ) => Promise<void>;
  deleteAiModelConfig: (id: string) => Promise<void>;
  setSelectedModel: (modelId: string | null) => void;
  loadAgents: (options?: { force?: boolean }) => Promise<void>;
  createAgent: (agent: AgentConfig) => Promise<AgentConfig | null>;
  updateAgent: (agent: AgentConfig) => Promise<AgentConfig | null>;
  deleteAgent: (agentId: string) => Promise<void>;
  aiCreateAgent: (payload: {
    requirement: string;
    name?: string;
    category?: string;
    description?: string;
    role_definition?: string;
    skill_ids?: string[];
    skill_prompts?: string[];
    enabled?: boolean;
    mcp_enabled?: boolean;
    enabled_mcp_ids?: string[];
    project_id?: string;
    project_root?: string;
  }) => Promise<AgentConfig | null>;
  setSelectedAgent: (agentId: string | null) => void;
  loadSystemContexts: () => Promise<void>;
  createSystemContext: (name: string, content: string, appIds?: string[]) => Promise<SystemContextResponse | null>;
  updateSystemContext: (id: string, name: string, content: string, appIds?: string[]) => Promise<SystemContextResponse | null>;
  deleteSystemContext: (id: string) => Promise<void>;
  activateSystemContext: (id: string) => Promise<void>;
  generateSystemContextDraft: (payload: {
    scene: string;
    style?: string;
    language?: string;
    output_format?: string;
    constraints?: string[];
    forbidden?: string[];
    candidate_count?: number;
    ai_model_config?: SystemContextModelConfigPayload;
  }) => Promise<SystemContextDraftGenerateResponse>;
  optimizeSystemContextDraft: (payload: {
    content: string;
    goal?: string;
    keep_intent?: boolean;
    ai_model_config?: SystemContextModelConfigPayload;
  }) => Promise<SystemContextDraftOptimizeResponse>;
  evaluateSystemContextDraft: (payload: {
    content: string;
  }) => Promise<SystemContextDraftEvaluateResponse>;
  loadApplications: () => Promise<void>;
  createApplication: (name: string, url: string, iconUrl?: string) => Promise<void>;
  updateApplication: (id: string, updates: Partial<Application>) => Promise<void>;
  deleteApplication: (id: string) => Promise<void>;
  setSelectedApplication: (appId: string | null) => void;
  setSystemContextAppAssociation: (contextId: string, appIds: string[]) => void;
  setAgentAppAssociation: (agentId: string, appIds: string[]) => void;
}
