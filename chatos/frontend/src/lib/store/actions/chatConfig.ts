// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ChatConfig } from '../../../types';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
}

export function createChatConfigActions({ set }: Deps) {
  return {
    updateChatConfig: async (config: Partial<ChatConfig>) => {
      set((state: ChatStoreDraft) => {
        state.chatConfig = { ...state.chatConfig, ...config };
      });
    },
  };
}
