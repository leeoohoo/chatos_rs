// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useOptionalApiClient } from '../lib/api/ApiClientContext';
import { useOptionalAuthStoreSelector } from '../lib/auth/authStore';

const TERMINAL_UI_STORAGE_KEY = 'chatos_terminal_ui_enabled';
const TERMINAL_UI_SETTING_EVENT = 'chatos:terminal-ui-setting-changed';

interface TerminalUiSettingState {
  resolved: boolean;
  enabled: boolean;
}

const normalizeTerminalUiEnabled = (value: unknown): boolean => {
  if (typeof value === 'boolean') {
    return value;
  }
  if (typeof value === 'number') {
    return value !== 0;
  }
  if (typeof value === 'string') {
    const normalized = value.trim().toLowerCase();
    if (normalized === 'false' || normalized === '0' || normalized === 'off') {
      return false;
    }
    if (normalized === 'true' || normalized === '1' || normalized === 'on') {
      return true;
    }
  }
  return true;
};

const readTerminalUiSettingValue = (settings: unknown): boolean => {
  if (!settings || typeof settings !== 'object' || Array.isArray(settings)) {
    return true;
  }
  return normalizeTerminalUiEnabled((settings as Record<string, unknown>).TERMINAL_UI_ENABLED);
};

const readStoredTerminalUiEnabled = (): boolean | null => {
  if (typeof window === 'undefined') {
    return null;
  }
  try {
    const stored = window.localStorage.getItem(TERMINAL_UI_STORAGE_KEY);
    if (stored === null) {
      return null;
    }
    return normalizeTerminalUiEnabled(stored);
  } catch {
    return null;
  }
};

export const writeStoredTerminalUiEnabled = (enabled: boolean) => {
  if (typeof window === 'undefined') {
    return;
  }
  try {
    window.localStorage.setItem(TERMINAL_UI_STORAGE_KEY, enabled ? 'true' : 'false');
  } catch {
    // ignore local storage errors
  }
};

export const emitTerminalUiSettingChanged = (enabled: boolean) => {
  if (typeof window === 'undefined') {
    return;
  }
  window.dispatchEvent(new CustomEvent(TERMINAL_UI_SETTING_EVENT, {
    detail: { enabled },
  }));
};

export const resolveTerminalUiEnabledFromResponse = (response: {
  effective?: Record<string, unknown>;
  settings?: Record<string, unknown>;
} | null | undefined): boolean => {
  return readTerminalUiSettingValue(response?.effective || response?.settings);
};

const buildInitialState = (): TerminalUiSettingState => {
  const stored = readStoredTerminalUiEnabled();
  if (stored === null) {
    return {
      resolved: false,
      enabled: false,
    };
  }
  return {
    resolved: true,
    enabled: stored,
  };
};

export const useTerminalUiSetting = () => {
  const apiClient = useOptionalApiClient();
  const userId = useOptionalAuthStoreSelector((state) => state.user?.id) || null;
  const initialized = useOptionalAuthStoreSelector((state) => state.initialized) === true;
  const [state, setState] = React.useState<TerminalUiSettingState>(() => buildInitialState());

  React.useEffect(() => {
    const apply = (enabled: boolean) => {
      writeStoredTerminalUiEnabled(enabled);
      setState({
        resolved: true,
        enabled,
      });
    };

    const handleSettingChanged = (event: Event) => {
      const detail = event instanceof CustomEvent
        ? event.detail as { enabled?: unknown } | undefined
        : undefined;
      apply(normalizeTerminalUiEnabled(detail?.enabled));
    };

    const handleStorage = (event: StorageEvent) => {
      if (event.key !== TERMINAL_UI_STORAGE_KEY || event.newValue === null) {
        return;
      }
      apply(normalizeTerminalUiEnabled(event.newValue));
    };

    window.addEventListener(TERMINAL_UI_SETTING_EVENT, handleSettingChanged as EventListener);
    window.addEventListener('storage', handleStorage);
    return () => {
      window.removeEventListener(TERMINAL_UI_SETTING_EVENT, handleSettingChanged as EventListener);
      window.removeEventListener('storage', handleStorage);
    };
  }, []);

  React.useEffect(() => {
    if (!initialized) {
      return;
    }
    if (!apiClient || !userId) {
      setState((prev) => (
        prev.resolved
          ? prev
          : {
            resolved: true,
            enabled: true,
          }
      ));
      return;
    }

    let cancelled = false;
    const stored = readStoredTerminalUiEnabled();
    if (stored === null) {
      setState({
        resolved: false,
        enabled: false,
      });
    }

    void apiClient.getUserSettings(userId)
      .then((response) => {
        if (cancelled) {
          return;
        }
        const enabled = resolveTerminalUiEnabledFromResponse(response);
        writeStoredTerminalUiEnabled(enabled);
        setState({
          resolved: true,
          enabled,
        });
      })
      .catch(() => {
        if (cancelled) {
          return;
        }
        setState({
          resolved: true,
          enabled: stored ?? true,
        });
      });

    return () => {
      cancelled = true;
    };
  }, [apiClient, initialized, userId]);

  return {
    terminalUiEnabled: state.enabled,
    terminalUiResolved: state.resolved,
  };
};
