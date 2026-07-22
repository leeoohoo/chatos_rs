// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import {
  useChatStoreContext,
  useChatStoreSelector,
} from '../../lib/store/ChatStoreContext';
import {
  hasOutstandingTaskRunnerCallbacks,
  useTaskRunnerCallbackReconciliation,
} from './useTaskRunnerCallbackReconciliation';

export const useCurrentTaskRunnerCallbackReconciliation = () => {
  const store = useChatStoreContext();
  const sessionId = useChatStoreSelector((state) => state.currentSessionId || null);
  const enabled = useChatStoreSelector((state) => (
    state.currentSessionId
      ? hasOutstandingTaskRunnerCallbacks(state.messages || [])
      : false
  ));
  const syncSessionMessages = useCallback(
    async (targetSessionId: string) => {
      await store.getState().syncSessionMessagesInBackground(targetSessionId);
    },
    [store],
  );

  useTaskRunnerCallbackReconciliation({
    enabled,
    sessionId,
    syncSessionMessages,
  });
};
