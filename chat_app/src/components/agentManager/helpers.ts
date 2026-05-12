import type { AgentConfig } from '../../types';
import type { AgentAiCreateFormData, AgentFormData } from './types';
export {
  buildGroupedConversationSessions,
  normalizeProjectId,
} from './sessionHelpers';

export const getDefaultAgentFormData = (): AgentFormData => ({
  name: '',
  description: '',
  category: '',
  roleDefinition: '',
  pluginSources: [],
  skillIds: [],
  enabled: true,
});

export const getDefaultAgentAiCreateFormData = (): AgentAiCreateFormData => ({
  requirement: '',
  modelConfigId: '',
  name: '',
  category: '',
  enabled: true,
});

export const toAgentFormData = (agent: AgentConfig): AgentFormData => ({
  id: agent.id,
  name: agent.name || '',
  description: agent.description || '',
  category: agent.category || '',
  roleDefinition: agent.role_definition || '',
  pluginSources: Array.isArray(agent.plugin_sources) ? agent.plugin_sources : [],
  skillIds: Array.isArray(agent.skill_ids) ? agent.skill_ids : [],
  enabled: agent.enabled !== false,
});

export const canSubmitAgentForm = (formData: AgentFormData): boolean => (
  formData.name.trim().length > 0 && formData.roleDefinition.trim().length > 0
);

export const canSubmitAiCreateAgentForm = (formData: AgentAiCreateFormData): boolean => (
  formData.requirement.trim().length > 0
);
