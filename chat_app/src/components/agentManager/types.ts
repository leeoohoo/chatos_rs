import type { AgentConfig, AiModelConfig, Message, Session } from '../../types';

export interface AgentConversationGroup {
  projectId: string;
  projectName: string;
  session: Session;
}

export interface AgentConversationState {
  open: boolean;
  loading: boolean;
  agent: AgentConfig | null;
  sessions: Session[];
  groupedSessions: AgentConversationGroup[];
  selectedSessionId: string | null;
  messages: Message[];
  messagesLoading: boolean;
  projectNames: Record<string, string>;
}

export interface AgentManagerProps {
  onClose: () => void;
  store?: () => {
    agents: AgentConfig[];
    aiModelConfigs: AiModelConfig[];
    loadAgents: (options?: { force?: boolean }) => Promise<void>;
    loadAiModelConfigs: (options?: { force?: boolean }) => Promise<void>;
    createAgent: (agent: AgentConfig) => Promise<AgentConfig | null>;
    updateAgent: (agent: AgentConfig) => Promise<AgentConfig | null>;
    deleteAgent: (agentId: string) => Promise<void>;
    aiCreateAgent: (payload: {
      model_config_id?: string;
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
  };
}

export interface AgentFormData {
  id?: string;
  name: string;
  description: string;
  category: string;
  roleDefinition: string;
  pluginSources: string[];
  skillIds: string[];
  enabled: boolean;
}

export interface AgentAiCreateFormData {
  requirement: string;
  modelConfigId: string;
  name: string;
  category: string;
  enabled: boolean;
}
