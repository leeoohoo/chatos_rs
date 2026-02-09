import type { Theme } from '../../../types';

interface Deps {
  set: any;
}

export function createUiActions({ set }: Deps) {
  return {
    toggleSidebar: () => {
      set((state: any) => {
        state.sidebarOpen = !state.sidebarOpen;
      });
    },

    setTheme: (theme: Theme) => {
      set((state: any) => {
        state.theme = theme;
      });
    },
  };
}
