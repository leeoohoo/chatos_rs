import type {
  AiModelConfig,
  MemoryAgent,
  MemorySkill,
  MemorySkillPlugin,
  Message,
  Session,
} from '../../types';

export interface AgentEditorState {
  id?: string;
  name: string;
  description: string;
  category: string;
  modelConfigId: string;
  roleDefinition: string;
  pluginSources: string[];
  skillIds: string[];
  enabled: boolean;
}

export const EMPTY_EDITOR: AgentEditorState = {
  name: '',
  description: '',
  category: '',
  modelConfigId: '',
  roleDefinition: '',
  pluginSources: [],
  skillIds: [],
  enabled: true,
};

export type AgentPageTranslate = (key: string) => string;

export interface AgentConversationGroup {
  projectId: string;
  projectName: string;
  session: Session;
}

export interface AgentConversationPanelState {
  open: boolean;
  agent: MemoryAgent | null;
  loading: boolean;
  sessions: Session[];
  sessionId: string | null;
  messages: Message[];
  messagesLoading: boolean;
  clearing: boolean;
  messagesPage: number;
  messagesPageSize: number;
  messagesHasMore: boolean;
  projectNames: Record<string, string>;
  groupedSessions: AgentConversationGroup[];
}

export interface AgentPluginPreviewState {
  open: boolean;
  loading: boolean;
  source: string;
  plugin: MemorySkillPlugin | null;
  skills: MemorySkill[];
}

export interface AgentSkillPreviewState {
  open: boolean;
  loading: boolean;
  skill: MemorySkill | null;
}

export interface AgentAiCreateState {
  open: boolean;
  requirement: string;
  name: string;
  category: string;
  enabled: boolean;
  modelConfigs: AiModelConfig[];
  modelsLoading: boolean;
  modelConfigId: string;
}
