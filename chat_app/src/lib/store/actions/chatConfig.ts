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
