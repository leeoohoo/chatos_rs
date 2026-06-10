import { useMemo } from 'react';

import type {
  AgentConfig,
  AiModelConfig,
  ContactRecord,
  Project,
  RemoteConnection,
  Session,
  Terminal,
} from '../../types';
import type { SessionChatState } from '../../lib/store/types';

import { resolveCurrentAgent } from './currentAgent';
import {
  buildSupportedFileTypes,
  resolveModelSupportFlags,
} from './viewHelpers';
import { useSessionHeaderMeta } from './useSessionHeaderMeta';
import { useI18n } from '../../i18n/I18nProvider';

interface UseChatInterfaceDerivedStateParams {
  currentSession: Session | null;
  contacts: ContactRecord[];
  agents: AgentConfig[];
  selectedAgentId: string | null;
  selectedModelId: string | null;
  aiModelConfigs: AiModelConfig[];
  activePanel: string;
  currentProject: Project | null;
  currentTerminal: Terminal | null;
  currentRemoteConnection: RemoteConnection | null;
  sessionChatState: Record<string, SessionChatState | undefined>;
}

export const useChatInterfaceDerivedState = ({
  currentSession,
  contacts,
  agents,
  selectedAgentId,
  selectedModelId,
  aiModelConfigs,
  activePanel,
  currentProject,
  currentTerminal,
  currentRemoteConnection,
  sessionChatState,
}: UseChatInterfaceDerivedStateParams) => {
  const { t } = useI18n();
  const { supportsImages, supportsReasoning } = useMemo(
    () => resolveModelSupportFlags(selectedModelId, aiModelConfigs),
    [aiModelConfigs, selectedModelId],
  );
  const supportedFileTypes = useMemo(
    () => buildSupportedFileTypes(supportsImages),
    [supportsImages],
  );
  const currentChatState = useMemo(() => (
    currentSession ? sessionChatState[currentSession.id] : undefined
  ), [currentSession, sessionChatState]);
  const currentAgent = useMemo(() => resolveCurrentAgent({
    currentSession,
    contacts,
    agents,
    selectedAgentId,
    fallbackAgentName: t('currentAgent.fallback'),
  }), [agents, contacts, currentSession, selectedAgentId, t]);
  const {
    currentContactName,
    currentContactId,
    headerTitle,
  } = useSessionHeaderMeta({
    currentSession,
    contacts,
    activePanel,
    currentProject,
    currentTerminal,
    currentRemoteConnection,
  });

  return {
    supportedFileTypes,
    supportsReasoning,
    currentChatState,
    currentAgent,
    currentContactName,
    currentContactId,
    headerTitle,
    chatIsLoading: currentChatState?.isLoading ?? false,
    chatIsStreaming: currentChatState?.isStreaming ?? false,
    chatIsStopping: currentChatState?.isStopping ?? false,
    chatStreamingPhase: currentChatState?.streamingPhase ?? null,
    chatStreamingPreviewText: currentChatState?.streamingPreviewText || '',
    runtimeContextRefreshNonce: currentChatState?.runtimeContextRefreshNonce || 0,
  };
};
