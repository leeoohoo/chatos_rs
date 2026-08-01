// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useOptionalApiClient } from '../lib/api/ApiClientContext';
import { useOptionalAuthStoreSelector } from '../lib/auth/authStore';

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

export const useLocalProjectCreationSetting = () => {
  const apiClient = useOptionalApiClient();
  const userId = useOptionalAuthStoreSelector((state) => state.user?.id) || null;
  const initialized = useOptionalAuthStoreSelector((state) => state.initialized) === true;
  const [state, setState] = React.useState({ resolved: false, enabled: false });

  React.useEffect(() => {
    if (!initialized) {
      return;
    }
    if (!apiClient || !userId) {
      setState({ resolved: true, enabled: false });
      return;
    }

    let cancelled = false;
    void apiClient.getUserSettings(userId)
      .then((response) => {
        if (!cancelled) {
          setState({
            resolved: true,
            enabled: resolveLocalProjectCreationEnabled(response),
          });
        }
      })
      .catch(() => {
        if (!cancelled) {
          setState({ resolved: true, enabled: false });
        }
      });

    return () => {
      cancelled = true;
    };
  }, [apiClient, initialized, userId]);

  return {
    localProjectCreationEnabled: state.enabled,
    localProjectCreationResolved: state.resolved,
  };
};
