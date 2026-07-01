// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
