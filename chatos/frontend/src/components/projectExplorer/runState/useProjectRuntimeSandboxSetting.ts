// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type ApiClient from '../../../lib/api/client';
import type { ProjectRuntimeEnvironmentResponse } from '../../../lib/api/client/types';
import type { Project } from '../../../types';

interface UseProjectRuntimeSandboxSettingOptions {
  client: ApiClient;
  project: Project | null;
  enabled?: boolean;
}

const getSandboxEnabled = (response: ProjectRuntimeEnvironmentResponse | null | undefined): boolean | null => {
  const environment = response?.environment;
  if (!environment) {
    return null;
  }
  if (typeof environment.sandbox_enabled === 'boolean') {
    return environment.sandbox_enabled;
  }
  if (typeof environment.sandboxEnabled === 'boolean') {
    return environment.sandboxEnabled;
  }
  return null;
};

const sourceTypeForBoundary = (project: Project | null): string => (
  (project?.sourceType || 'local').trim().toLowerCase()
);

export const isProjectRuntimeSandboxConfigurable = (project: Project | null): boolean => {
  const sourceType = sourceTypeForBoundary(project);
  return sourceType === 'local' || sourceType === 'local_connector';
};

const formatSandboxSettingError = (fallback: string, error: unknown): string => {
  if (error instanceof Error && error.message.trim()) {
    return `${fallback}: ${error.message}`;
  }
  return fallback;
};

export const useProjectRuntimeSandboxSetting = ({
  client,
  project,
  enabled = true,
}: UseProjectRuntimeSandboxSettingOptions) => {
  const { t } = useI18n();
  const sandboxToggleVisible = useMemo(
    () => Boolean(project?.id) && isProjectRuntimeSandboxConfigurable(project),
    [project?.id, project?.sourceType],
  );
  const [sandboxEnabled, setSandboxEnabled] = useState<boolean | null>(null);
  const [sandboxLoading, setSandboxLoading] = useState(false);
  const [sandboxSaving, setSandboxSaving] = useState(false);
  const [sandboxError, setSandboxError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled || !sandboxToggleVisible || !project?.id) {
      setSandboxEnabled(null);
      setSandboxLoading(false);
      setSandboxSaving(false);
      setSandboxError(null);
      return undefined;
    }

    let cancelled = false;
    setSandboxLoading(true);
    setSandboxError(null);

    void client.getProjectRuntimeEnvironment(project.id)
      .then((response) => {
        if (cancelled) {
          return;
        }
        setSandboxEnabled(getSandboxEnabled(response));
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setSandboxError(formatSandboxSettingError(t('runSettings.error.loadSandboxSettingFailed'), error));
        setSandboxEnabled(null);
      })
      .finally(() => {
        if (!cancelled) {
          setSandboxLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [client, enabled, project?.id, sandboxToggleVisible, t]);

  const updateSandboxEnabled = useCallback(async (nextEnabled: boolean) => {
    if (!project?.id || !sandboxToggleVisible) {
      return;
    }

    const previousEnabled = sandboxEnabled;
    setSandboxSaving(true);
    setSandboxError(null);
    setSandboxEnabled(nextEnabled);

    try {
      const response = await client.updateProjectRuntimeEnvironmentSettings(project.id, {
        sandbox_enabled: nextEnabled,
      });
      setSandboxEnabled(getSandboxEnabled(response) ?? nextEnabled);
    } catch (error) {
      setSandboxEnabled(previousEnabled);
      setSandboxError(formatSandboxSettingError(t('runSettings.error.saveSandboxSettingFailed'), error));
    } finally {
      setSandboxSaving(false);
    }
  }, [client, project?.id, sandboxEnabled, sandboxToggleVisible, t]);

  return {
    sandboxToggleVisible,
    sandboxEnabled,
    sandboxLoading,
    sandboxSaving,
    sandboxError,
    updateSandboxEnabled,
  };
};
