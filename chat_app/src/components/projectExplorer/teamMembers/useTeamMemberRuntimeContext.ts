import { useCallback, useEffect, useRef, useState } from 'react';

import {
  resolveSessionProjectScopeId,
} from '../../../features/contactSession/sessionResolver';
import type { TurnRuntimeSnapshotLookupResponse } from '../../../lib/api/client/types';
import {
  getCachedRuntimeContextData,
  loadRuntimeContextSnapshot,
  markRuntimeContextStale,
} from '../../../lib/runtimeContext/cache';
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
  const latestSessionIdRef = useRef<string | null>(null);
  const refreshNonceRef = useRef(runtimeContextRefreshNonce);
  const lastRefreshSignatureRef = useRef<string | null>(null);
  const latestOpenRequestSeqRef = useRef(0);

  refreshNonceRef.current = runtimeContextRefreshNonce;

  const loadLatestRuntimeContext = useCallback(async (
    sessionId: string,
    options?: { force?: boolean; silent?: boolean },
  ) => {
    if (!sessionId) {
      return;
    }
    latestSessionIdRef.current = sessionId;
    if (!options?.silent) {
      setRuntimeContextLoading(true);
    }
    setRuntimeContextError(null);
    try {
      const payload = await loadRuntimeContextSnapshot(apiClient, sessionId, options);
      if (latestSessionIdRef.current !== sessionId) {
        return;
      }
      setRuntimeContextData(payload);
    } catch (error) {
      console.error('Failed to load turn runtime context in team pane:', error);
      if (latestSessionIdRef.current === sessionId) {
        setRuntimeContextError(error instanceof Error ? error.message : '加载上下文失败');
      }
    } finally {
      if (latestSessionIdRef.current === sessionId && !options?.silent) {
        setRuntimeContextLoading(false);
      }
    }
  }, [apiClient]);

  const handleOpenRuntimeContext = useCallback(async (contact: ContactItem) => {
    const requestSeq = latestOpenRequestSeqRef.current + 1;
    latestOpenRequestSeqRef.current = requestSeq;
    setOpeningRuntimeContextContactId(contact.id);
    setSelectedContactId(contact.id);
    try {
      const sessionId = await ensureContactSession(contact);
      if (latestOpenRequestSeqRef.current !== requestSeq) {
        return;
      }
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
      latestSessionIdRef.current = sessionId;
      setRuntimeContextOpen(true);
      setRuntimeContextSessionId(sessionId);
      setRuntimeContextData(getCachedRuntimeContextData(apiClient, sessionId));
    } finally {
      setOpeningRuntimeContextContactId((prev) => (prev === contact.id ? null : prev));
    }
  }, [
    apiClient,
    ensureContactSession,
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
    markRuntimeContextStale(apiClient, runtimeContextSessionId);
    void loadLatestRuntimeContext(runtimeContextSessionId, { force: true });
  }, [apiClient, loadLatestRuntimeContext, runtimeContextSessionId]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      lastRefreshSignatureRef.current = null;
      return;
    }
    setRuntimeContextData(getCachedRuntimeContextData(apiClient, runtimeContextSessionId));
    lastRefreshSignatureRef.current = `${runtimeContextSessionId}:${refreshNonceRef.current}`;
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [
    apiClient,
    loadLatestRuntimeContext,
    runtimeContextOpen,
    runtimeContextSessionId,
  ]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      return;
    }
    const signature = `${runtimeContextSessionId}:${runtimeContextRefreshNonce}`;
    if (lastRefreshSignatureRef.current === signature) {
      return;
    }
    lastRefreshSignatureRef.current = signature;
    markRuntimeContextStale(apiClient, runtimeContextSessionId);
    setRuntimeContextData(getCachedRuntimeContextData(apiClient, runtimeContextSessionId));
    void loadLatestRuntimeContext(runtimeContextSessionId, { silent: true });
  }, [
    apiClient,
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
