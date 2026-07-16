// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type ApiClient from '../../api/client';
import type { ChatStoreGet, ChatStoreSet } from '../types';

export const createStopMessageHandler = ({
  set,
  get,
  client,
}: {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}) => async (): Promise<void> => {
  const { currentSessionId, sessionChatState } = get();
  if (!currentSessionId) {
    return;
  }
  const current = sessionChatState[currentSessionId];
  const turnId = current?.activeTurnId || null;
  if (!current?.isLoading || !turnId || current.isStopping) {
    return;
  }

  set((state) => {
    const active = state.sessionChatState[currentSessionId];
    if (active) {
      active.isStopping = true;
    }
  });
  try {
    const response = await client.stopChat(currentSessionId, turnId);
    if (!response.success) {
      throw new Error(response.message || '没有找到可停止的运行轮次');
    }
  } catch (error) {
    set((state) => {
      const active = state.sessionChatState[currentSessionId];
      if (active) {
        active.isStopping = false;
      }
      state.error = error instanceof Error ? error.message : '停止运行失败';
    });
    throw error;
  }
};
