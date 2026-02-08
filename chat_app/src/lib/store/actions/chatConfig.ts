import type { ChatConfig } from '../../../types';

interface Deps {
  set: any;
  get: any;
}

export function createChatConfigActions({ set }: Deps) {
  return {
    updateChatConfig: async (config: Partial<ChatConfig>) => {
      set((state: any) => {
        state.chatConfig = { ...state.chatConfig, ...config };
      });
    },
  };
}