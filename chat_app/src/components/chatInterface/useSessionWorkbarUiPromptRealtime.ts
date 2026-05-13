import { useCallback, useEffect, useRef } from 'react';

import { useRealtimeInvalidationQueue } from '../../lib/realtime/invalidationQueue';
import type { RealtimeUiPromptPayloadWrapper } from '../../lib/realtime/types';
import { useConversationUiPromptRealtime } from '../../lib/realtime/useConversationUiPromptRealtime';
import { toUiPromptPanelFromRealtimePayload } from './panelTransforms';
import {
  removePendingUiPromptCachePanel,
  upsertPendingUiPromptCachePanel,
} from './pendingUiPromptCache';
import type { SessionWorkbarApiClient } from './useSessionWorkbarPanels.types';
import type { UiPromptPanelState } from '../../lib/store/types';

interface UseSessionWorkbarUiPromptRealtimeArgs {
  apiClient: SessionWorkbarApiClient;
  enabled: boolean;
  sessionId: string | null;
  preferRealtimeSync: boolean;
  uiPromptHistoryOpen: boolean;
  loadUiPromptHistory?: (sessionId: string, force?: boolean) => Promise<void>;
  markUiPromptHistoryStale?: (sessionId: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
}

export const useSessionWorkbarUiPromptRealtime = ({
  apiClient,
  enabled,
  sessionId,
  preferRealtimeSync,
  uiPromptHistoryOpen,
  loadUiPromptHistory,
  markUiPromptHistoryStale,
  upsertUiPromptPanel,
  removeUiPromptPanel,
}: UseSessionWorkbarUiPromptRealtimeArgs) => {
  const reloadKeyRef = useRef('');
  const sessionIdRef = useRef(sessionId);
  const loadUiPromptHistoryRef = useRef(loadUiPromptHistory);
  const markUiPromptHistoryStaleRef = useRef(markUiPromptHistoryStale);
  const preferRealtimeSyncRef = useRef(preferRealtimeSync);
  const uiPromptHistoryOpenRef = useRef(uiPromptHistoryOpen);

  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);

  useEffect(() => {
    loadUiPromptHistoryRef.current = loadUiPromptHistory;
  }, [loadUiPromptHistory]);

  useEffect(() => {
    markUiPromptHistoryStaleRef.current = markUiPromptHistoryStale;
  }, [markUiPromptHistoryStale]);

  useEffect(() => {
    preferRealtimeSyncRef.current = preferRealtimeSync;
  }, [preferRealtimeSync]);

  useEffect(() => {
    uiPromptHistoryOpenRef.current = uiPromptHistoryOpen;
  }, [uiPromptHistoryOpen]);

  const reloadQueue = useRealtimeInvalidationQueue<RealtimeUiPromptPayloadWrapper>({
    onExecute: async () => {
      const currentSessionId = sessionIdRef.current;
      const loadHistory = loadUiPromptHistoryRef.current;
      if (!loadHistory || !currentSessionId) {
        return;
      }
      if (preferRealtimeSyncRef.current || uiPromptHistoryOpenRef.current) {
        await loadHistory(currentSessionId, true);
        return;
      }
      markUiPromptHistoryStaleRef.current?.(currentSessionId);
    },
  });

  const handleEvent = useCallback(async (payload: RealtimeUiPromptPayloadWrapper) => {
    if (payload.action === 'prompt_required') {
      const panel = toUiPromptPanelFromRealtimePayload(payload);
      if (panel) {
        upsertPendingUiPromptCachePanel(apiClient, panel);
        upsertUiPromptPanel(panel);
      }
    }

    if (payload.action === 'prompt_resolved') {
      const promptId = typeof payload.prompt_id === 'string' ? payload.prompt_id.trim() : '';
      if (promptId) {
        removePendingUiPromptCachePanel(apiClient, promptId, sessionId || undefined);
        removeUiPromptPanel(promptId, sessionId || undefined);
      }
    }

    if (!loadUiPromptHistory || !sessionId) {
      return;
    }

    const reloadKey = [
      payload.action,
      payload.prompt_id || '',
      payload.status || '',
    ].join(':');
    if (reloadKeyRef.current === reloadKey) {
      return;
    }
    reloadKeyRef.current = reloadKey;
    window.setTimeout(() => {
      if (reloadKeyRef.current === reloadKey) {
        reloadKeyRef.current = '';
      }
    }, 300);

    reloadQueue.run(payload);
  }, [
    apiClient,
    loadUiPromptHistory,
    reloadQueue,
    removeUiPromptPanel,
    sessionId,
    upsertUiPromptPanel,
  ]);

  useConversationUiPromptRealtime({
    sessionId,
    enabled: enabled && Boolean(sessionId),
    onEvent: handleEvent,
  });
};
