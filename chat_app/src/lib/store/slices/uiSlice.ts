import type { Theme } from '../../../types';

export interface UiSliceState {
  sidebarOpen: boolean;
  theme: Theme;
  error: string | null;
}

export const uiInitialState: UiSliceState = {
  sidebarOpen: true,
  theme: 'light',
  error: null,
};

export interface UiSliceActions {
  toggleSidebar: () => void;
  setTheme: (theme: Theme) => void;
  setError: (error: string | null) => void;
  clearError: () => void;
}
