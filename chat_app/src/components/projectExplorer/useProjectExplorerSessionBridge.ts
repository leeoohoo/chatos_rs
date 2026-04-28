import { useMemo } from 'react';
import { shallow } from 'zustand/shallow';

import { apiClient as globalApiClient } from '../../lib/api/client';
import {
  useChatApiClientFromContext,
  useChatStoreSelector,
} from '../../lib/store/ChatStoreContext';
import type { Project } from '../../types';
import { useContactSessionResolver } from '../../features/contactSession/useContactSessionResolver';
import { useProjectRunnerScriptGenerator } from './useProjectRunnerScriptGenerator';

interface UseProjectExplorerSessionBridgeParams {
  project: Project | null;
}

export const useProjectExplorerSessionBridge = ({
  project,
}: UseProjectExplorerSessionBridgeParams) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const {
    currentSession,
    sessions,
    createSession,
    selectSession,
    sendMessage,
    selectedModelId,
  } = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    sessions: state.sessions,
    createSession: state.createSession,
    selectSession: state.selectSession,
    sendMessage: state.sendMessage,
    selectedModelId: state.selectedModelId,
  }), shallow);

  const { ensureContactSession } = useContactSessionResolver({
    sessions: sessions || [],
    currentSession,
    createSession,
    apiClient: client,
    defaultProjectId: project?.id || null,
  });

  const handleGenerateRunnerScriptForContact = useProjectRunnerScriptGenerator({
    project,
    currentSessionId: currentSession?.id,
    selectedModelId,
    ensureContactSession,
    selectSession,
    sendMessage,
  });

  return {
    client,
    handleGenerateRunnerScriptForContact,
  };
};
