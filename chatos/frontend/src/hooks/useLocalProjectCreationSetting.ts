// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useOptionalApiClient } from '../lib/api/ApiClientContext';
import { useOptionalAuthStoreSelector } from '../lib/auth/authStore';
import { localRuntimeBridgeAvailable } from '../lib/api/localRuntime/bridge';

const LOCAL_PROJECT_CREATION_REFRESH_INTERVAL_MS = 10_000;

export const normalizeLocalProjectCreationEnabled = (value: unknown): boolean => {
  if (typeof value === 'boolean') {
    return value;
  }
  if (typeof value === 'number') {
    return value !== 0;
  }
  if (typeof value === 'string') {
    return ['1', 'true', 'yes', 'on'].includes(value.trim().toLowerCase());
  }
  return false;
};

export const resolveLocalProjectCreationEnabled = (response: {
  effective?: Record<string, unknown>;
  settings?: Record<string, unknown>;
} | null | undefined): boolean => {
  const settings = response?.effective || response?.settings;
  return normalizeLocalProjectCreationEnabled(settings?.LOCAL_PROJECT_CREATION_ENABLED);
};

export const resolveLocalProjectCreationAvailable = (
  response: Parameters<typeof resolveLocalProjectCreationEnabled>[0],
  desktopRuntimeAvailable: boolean,
): boolean => desktopRuntimeAvailable && resolveLocalProjectCreationEnabled(response);

export const useLocalProjectCreationSetting = () => {
  const apiClient = useOptionalApiClient();
  const userId = useOptionalAuthStoreSelector((state) => state.user?.id) || null;
  const initialized = useOptionalAuthStoreSelector((state) => state.initialized) === true;
  const [state, setState] = React.useState({ resolved: false, enabled: false });

  React.useEffect(() => {
    if (!initialized) {
      return;
    }
    const desktopRuntimeAvailable = localRuntimeBridgeAvailable();
    if (!apiClient || !userId || !desktopRuntimeAvailable) {
      setState({ resolved: true, enabled: false });
      return;
    }

    let cancelled = false;
    const refresh = () => {
      void apiClient.getUserSettings(userId)
        .then((response) => {
          if (!cancelled) {
            setState({
              resolved: true,
              enabled: resolveLocalProjectCreationAvailable(response, desktopRuntimeAvailable),
            });
          }
        })
        .catch(() => {
          if (!cancelled) {
            setState({ resolved: true, enabled: false });
          }
        });
    };
    const refreshWhenVisible = () => {
      if (document.visibilityState === 'visible') {
        refresh();
      }
    };

    refresh();
    const intervalId = window.setInterval(refresh, LOCAL_PROJECT_CREATION_REFRESH_INTERVAL_MS);
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', refreshWhenVisible);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
      window.removeEventListener('focus', refresh);
      document.removeEventListener('visibilitychange', refreshWhenVisible);
    };
  }, [apiClient, initialized, userId]);

  return {
    localProjectCreationEnabled: state.enabled,
    localProjectCreationResolved: state.resolved,
  };
};
