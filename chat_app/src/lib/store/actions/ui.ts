import type { Theme } from '../../../types';
import type { ChatStoreDraft, ChatStoreSet } from '../types';

interface Deps {
  set: ChatStoreSet;
}

export function createUiActions({ set }: Deps) {
  return {
    toggleSidebar: () => {
      set((state: ChatStoreDraft) => {
        state.sidebarOpen = !state.sidebarOpen;
      });
    },

    setTheme: (theme: Theme) => {
      set((state: ChatStoreDraft) => {
        state.theme = theme;
      });
    },
  };
}
