// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export const getThemeColors = (theme: 'light' | 'dark') => {
  if (theme === 'dark') {
    return {
      background: '#0f172a',
      foreground: '#e2e8f0',
      cursor: '#f8fafc',
      selection: 'rgba(148, 163, 184, 0.35)',
      black: '#0f172a',
      red: '#f87171',
      green: '#34d399',
      yellow: '#fbbf24',
      blue: '#60a5fa',
      magenta: '#c084fc',
      cyan: '#22d3ee',
      white: '#e2e8f0',
      brightBlack: '#334155',
      brightRed: '#fca5a5',
      brightGreen: '#6ee7b7',
      brightYellow: '#fde68a',
      brightBlue: '#93c5fd',
      brightMagenta: '#d8b4fe',
      brightCyan: '#67e8f9',
      brightWhite: '#f8fafc',
    };
  }
  return {
    background: '#ffffff',
    foreground: '#0f172a',
    cursor: '#0f172a',
    selection: 'rgba(59, 130, 246, 0.25)',
    black: '#0f172a',
    red: '#dc2626',
    green: '#16a34a',
    yellow: '#d97706',
    blue: '#2563eb',
    magenta: '#7c3aed',
    cyan: '#0891b2',
    white: '#e2e8f0',
    brightBlack: '#475569',
    brightRed: '#ef4444',
    brightGreen: '#22c55e',
    brightYellow: '#f59e0b',
    brightBlue: '#3b82f6',
    brightMagenta: '#8b5cf6',
    brightCyan: '#06b6d4',
    brightWhite: '#f8fafc',
  };
};

export const toXtermTheme = (palette: ReturnType<typeof getThemeColors>) => ({
  background: palette.background,
  foreground: palette.foreground,
  cursor: palette.cursor,
  selectionBackground: palette.selection,
  black: palette.black,
  red: palette.red,
  green: palette.green,
  yellow: palette.yellow,
  blue: palette.blue,
  magenta: palette.magenta,
  cyan: palette.cyan,
  white: palette.white,
  brightBlack: palette.brightBlack,
  brightRed: palette.brightRed,
  brightGreen: palette.brightGreen,
  brightYellow: palette.brightYellow,
  brightBlue: palette.brightBlue,
  brightMagenta: palette.brightMagenta,
  brightCyan: palette.brightCyan,
  brightWhite: palette.brightWhite,
});
