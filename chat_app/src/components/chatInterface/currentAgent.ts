import { readSessionRuntimeFromMetadata } from '../../lib/store/helpers/sessionRuntime';
import type { AgentConfig, ContactRecord, Session } from '../../types';

interface ResolveCurrentAgentParams {
  currentSession: Session | null | undefined;
  contacts: ContactRecord[] | null | undefined;
  agents: AgentConfig[] | null | undefined;
  selectedAgentId: string | null | undefined;
  fallbackAgentName?: string;
}

const trimString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const createFallbackAgent = (
  agentId: string,
  name: string,
): AgentConfig => {
  const now = new Date();
  return {
    id: agentId,
    name,
    description: '',
    ai_model_config_id: '',
    enabled: true,
    role_definition: '',
    skills: [],
    skill_ids: [],
    default_skill_ids: [],
    plugin_sources: [],
    runtime_plugins: [],
    runtime_skills: [],
    mcp_policy: null,
    project_policy: null,
    createdAt: now,
    updatedAt: now,
  };
};

export const resolveCurrentAgent = ({
  currentSession,
  contacts,
  agents,
  selectedAgentId,
  fallbackAgentName = 'Current agent',
}: ResolveCurrentAgentParams): AgentConfig | null => {
  const runtime = readSessionRuntimeFromMetadata(currentSession?.metadata);
  const runtimeContactId = trimString(runtime?.contactId);
  const runtimeAgentId = trimString(runtime?.contactAgentId);
  const sessionTitle = trimString(currentSession?.title);

  const matchedContact = Array.isArray(contacts)
    ? contacts.find((contact) => {
      const contactId = trimString(contact?.id);
      const contactAgentId = trimString(contact?.agentId);
      const contactName = trimString(contact?.name);
      if (runtimeContactId && contactId === runtimeContactId) {
        return true;
      }
      if (runtimeAgentId && contactAgentId === runtimeAgentId) {
        return true;
      }
      return !runtimeAgentId
        && !runtimeContactId
        && !!sessionTitle
        && contactName === sessionTitle;
    }) || null
    : null;

  const matchedContactAgentId = trimString(matchedContact?.agentId);
  const matchedContactName = trimString(matchedContact?.name);
  const agentId = trimString(selectedAgentId) || runtimeAgentId || matchedContactAgentId;
  if (!agentId) {
    return null;
  }

  const matchedAgent = Array.isArray(agents)
    ? (agents.find((agent) => agent?.id === agentId) || null)
    : null;
  if (matchedAgent) {
    return matchedAgent;
  }

  return createFallbackAgent(
    agentId,
    matchedContactName || sessionTitle || fallbackAgentName,
  );
};
