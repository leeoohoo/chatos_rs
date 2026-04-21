import { useCallback, useEffect, useState } from 'react';

import {
  resolveSessionProjectScopeId,
} from '../../../features/contactSession/sessionResolver';
import type { TurnRuntimeSnapshotLookupResponse } from '../../../lib/api/client/types';
import type { Session } from '../../../types';
import type { ContactItem } from './types';

interface RuntimeContextApiClient {
  getConversationLatestTurnRuntimeContext: (
    sessionId: string,
  ) => Promise<TurnRuntimeSnapshotLookupResponse>;
}

interface UseTeamMemberRuntimeContextOptions {
  apiClient: RuntimeContextApiClient;
  sessions: Session[];
  normalizedProjectId: string;
  runtimeContextRefreshNonce: number;
  ensureContactSession: (contact: ContactItem) => Promise<string | null>;
  setSelectedContactId: (contactId: string | null) => void;
}

export const useTeamMemberRuntimeContext = ({
  apiClient,
  sessions,
  normalizedProjectId,
  runtimeContextRefreshNonce,
  ensureContactSession,
  setSelectedContactId,
}: UseTeamMemberRuntimeContextOptions) => {
  const [runtimeContextOpen, setRuntimeContextOpen] = useState(false);
  const [runtimeContextSessionId, setRuntimeContextSessionId] = useState<string | null>(null);
  const [runtimeContextData, setRuntimeContextData] =
    useState<TurnRuntimeSnapshotLookupResponse | null>(null);
  const [runtimeContextLoading, setRuntimeContextLoading] = useState(false);
  const [runtimeContextError, setRuntimeContextError] = useState<string | null>(null);
  const [openingRuntimeContextContactId, setOpeningRuntimeContextContactId] = useState<string | null>(null);

  const loadLatestRuntimeContext = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextLoading(true);
    setRuntimeContextError(null);
    try {
      const payload = await apiClient.getConversationLatestTurnRuntimeContext(sessionId);
      setRuntimeContextData(payload);
    } catch (error) {
      console.error('Failed to load turn runtime context in team pane:', error);
      setRuntimeContextError(error instanceof Error ? error.message : '加载上下文失败');
    } finally {
      setRuntimeContextLoading(false);
    }
  }, [apiClient]);

  const handleOpenRuntimeContext = useCallback(async (contact: ContactItem) => {
    setOpeningRuntimeContextContactId(contact.id);
    setSelectedContactId(contact.id);
    try {
      const sessionId = await ensureContactSession(contact);
      if (!sessionId) {
        return;
      }
      const targetSession = sessions.find((item) => item.id === sessionId) || null;
      if (targetSession && resolveSessionProjectScopeId(targetSession) !== normalizedProjectId) {
        setRuntimeContextError('检测到跨项目会话，已阻止加载上下文');
        setRuntimeContextOpen(false);
        return;
      }
      if (runtimeContextOpen && runtimeContextSessionId === sessionId) {
        setRuntimeContextOpen(false);
        return;
      }
      setRuntimeContextOpen(true);
      setRuntimeContextSessionId(sessionId);
      setRuntimeContextData(null);
      await loadLatestRuntimeContext(sessionId);
    } finally {
      setOpeningRuntimeContextContactId((prev) => (prev === contact.id ? null : prev));
    }
  }, [
    ensureContactSession,
    loadLatestRuntimeContext,
    normalizedProjectId,
    runtimeContextOpen,
    runtimeContextSessionId,
    sessions,
    setSelectedContactId,
  ]);

  const handleRefreshRuntimeContext = useCallback(() => {
    if (!runtimeContextSessionId) {
      return;
    }
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [loadLatestRuntimeContext, runtimeContextSessionId]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      return;
    }
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [
    loadLatestRuntimeContext,
    runtimeContextOpen,
    runtimeContextRefreshNonce,
    runtimeContextSessionId,
  ]);

  return {
    runtimeContextOpen,
    setRuntimeContextOpen,
    runtimeContextSessionId,
    runtimeContextData,
    runtimeContextLoading,
    runtimeContextError,
    setRuntimeContextError,
    openingRuntimeContextContactId,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  };
};
