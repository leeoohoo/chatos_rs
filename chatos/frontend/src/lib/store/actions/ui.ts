// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
