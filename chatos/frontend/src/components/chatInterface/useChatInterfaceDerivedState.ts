// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';

import type {
  AiModelConfig,
  ContactRecord,
  Project,
  RemoteConnection,
  Session,
  Terminal,
} from '../../types';
import type { SessionChatState } from '../../lib/store/types';

import {
  buildSupportedFileTypes,
  resolveModelSupportFlags,
} from './viewHelpers';
import { useSessionHeaderMeta } from './useSessionHeaderMeta';

interface UseChatInterfaceDerivedStateParams {
  currentSession: Session | null;
  contacts: ContactRecord[];
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
  selectedModelId,
  aiModelConfigs,
  activePanel,
  currentProject,
  currentTerminal,
  currentRemoteConnection,
  sessionChatState,
}: UseChatInterfaceDerivedStateParams) => {
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
    currentContactName,
    currentContactId,
    headerTitle,
    runtimeContextRefreshNonce: currentChatState?.runtimeContextRefreshNonce || 0,
    isLoading: currentChatState?.isLoading === true,
    isStreaming: currentChatState?.isStreaming === true,
    isStopping: currentChatState?.isStopping === true,
    streamingPhase: currentChatState?.streamingPhase ?? null,
    streamingPreviewText: currentChatState?.streamingPreviewText || '',
  };
};
