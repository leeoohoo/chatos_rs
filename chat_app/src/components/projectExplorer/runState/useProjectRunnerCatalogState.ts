// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type ApiClient from '../../../lib/api/client';
import type {
  Project,
  ProjectRunEnvironment,
  ProjectRunTarget,
} from '../../../types';
import {
  buildCustomToolchainDrafts,
  buildEnvironmentHints,
  buildEnvPreview,
  buildEnvVarsPlaceholder,
  buildMissingToolchainKinds,
  buildSelectedToolchainOptions,
  resolveCommandPreview,
  serializeEnvVarsDraft,
} from './projectRunnerEnvironmentState';
import { resolveProjectRunnerStatus, resolveProjectRunTargetSelection } from './projectRunnerCatalogState';
import { useProjectRunnerCatalogLifecycle } from './useProjectRunnerCatalogLifecycle';
import { useProjectRunnerEnvironmentMutations } from './useProjectRunnerEnvironmentMutations';

interface UseProjectRunnerCatalogStateOptions {
  client: ApiClient;
  project: Project | null;
  enabled?: boolean;
}

export const useProjectRunnerCatalogState = ({
  client,
  project,
  enabled = true,
}: UseProjectRunnerCatalogStateOptions) => {
  const { t } = useI18n();
  const [selectedRunTargetId, setSelectedRunTargetId] = useState<string | null>(null);
  const [runTargets, setRunTargets] = useState<ProjectRunTarget[]>([]);
  const [runCatalogLoading, setRunCatalogLoading] = useState(false);
  const [runCatalogError, setRunCatalogError] = useState<string | null>(null);
  const [runEnvironment, setRunEnvironment] = useState<ProjectRunEnvironment | null>(null);
  const [runEnvironmentLoading, setRunEnvironmentLoading] = useState(false);
  const [runEnvironmentError, setRunEnvironmentError] = useState<string | null>(null);
  const [customToolchainDrafts, setCustomToolchainDrafts] = useState<Record<string, string>>({});
  const [envVarsDraft, setEnvVarsDraft] = useState('');

  const applyRunCatalog = useCallback((catalog: { targets: ProjectRunTarget[]; errorMessage?: string | null; defaultTargetId?: string | null }) => {
    setRunTargets(catalog.targets);
    setRunCatalogError(catalog.errorMessage || null);
    setSelectedRunTargetId((prev) => resolveProjectRunTargetSelection({
      currentSelectedRunTargetId: prev,
      targets: catalog.targets,
      defaultTargetId: catalog.defaultTargetId,
    }));
  }, []);

  const {
    loadRunCatalog,
    loadRunEnvironment,
    refreshRunnerState,
    invalidateRunnerCatalogState,
    selectRunTarget,
  } = useProjectRunnerCatalogLifecycle({
    client,
    project,
    enabled,
    setRunTargets,
    setRunCatalogLoading,
    setRunCatalogError,
    setRunEnvironment,
    setRunEnvironmentLoading,
    setRunEnvironmentError,
    setSelectedRunTargetId,
    setCustomToolchainDrafts,
    setEnvVarsDraft,
    applyRunCatalog,
  });

  const selectedRunTarget = useMemo(
    () => runTargets.find((item) => item.id === selectedRunTargetId) || runTargets[0] || null,
    [runTargets, selectedRunTargetId],
  );

  const availableToolchainKinds = useMemo(() => (
    selectedRunTarget?.requiredToolchains || []
  ), [selectedRunTarget?.requiredToolchains]);

  useEffect(() => {
    setCustomToolchainDrafts(
      buildCustomToolchainDrafts(runEnvironment, availableToolchainKinds),
    );
  }, [availableToolchainKinds, runEnvironment]);

  const selectedToolchainOptions = useMemo(
    () => buildSelectedToolchainOptions(runEnvironment, availableToolchainKinds),
    [availableToolchainKinds, runEnvironment],
  );

  const missingToolchainKinds = useMemo(
    () => buildMissingToolchainKinds(availableToolchainKinds, runEnvironment),
    [availableToolchainKinds, runEnvironment],
  );

  const updateCustomToolchainDraft = useCallback((kind: string, value: string) => {
    const normalizedKind = kind.trim();
    if (!normalizedKind) {
      return;
    }
    setCustomToolchainDrafts((prev) => ({
      ...prev,
      [normalizedKind]: value,
    }));
  }, []);

  const {
    updateSelectedToolchain,
    saveCustomToolchain,
    saveEnvVarsDraft,
    setTerminalUiEnabled,
  } = useProjectRunnerEnvironmentMutations({
    client,
    project,
    enabled,
    runEnvironment,
    customToolchainDrafts,
    envVarsDraft,
    setRunEnvironment,
    setRunEnvironmentError,
    setEnvVarsDraft,
    loadRunEnvironment,
  });

  const commandPreview = useMemo(() => {
    const command = resolveCommandPreview(selectedRunTarget?.command || '', selectedToolchainOptions);
    const envPrefix = serializeEnvVarsDraft(runEnvironment?.envVars || {});
    if (!envPrefix) {
      return command;
    }
    return `${envPrefix}\n${command}`.trim();
  }, [runEnvironment?.envVars, selectedRunTarget?.command, selectedToolchainOptions]);

  const envPreview = useMemo(
    () => buildEnvPreview(runEnvironment?.envVars || {}, selectedToolchainOptions),
    [runEnvironment?.envVars, selectedToolchainOptions],
  );

  const environmentHints = useMemo(
    () => buildEnvironmentHints(selectedRunTarget, selectedToolchainOptions, t),
    [selectedRunTarget, selectedToolchainOptions, t],
  );

  const envVarsPlaceholder = useMemo(
    () => buildEnvVarsPlaceholder(selectedRunTarget),
    [selectedRunTarget],
  );

  const runStatus = useMemo(() => {
    return resolveProjectRunnerStatus({
      enabled,
      projectId: project?.id || null,
      loading: runCatalogLoading,
      errorMessage: runCatalogError,
      targetCount: runTargets.length,
    });
  }, [enabled, project?.id, runCatalogError, runCatalogLoading, runTargets.length]);

  useEffect(() => {
    if (!enabled) {
      return;
    }
    if (runTargets.length === 0) {
      setSelectedRunTargetId(null);
      return;
    }
    setSelectedRunTargetId((prev) => resolveProjectRunTargetSelection({
      currentSelectedRunTargetId: prev,
      targets: runTargets,
      defaultTargetId: null,
    }));
  }, [enabled, runTargets]);

  return {
    runStatus,
    runTargets,
    runCatalogLoading,
    runCatalogError,
    runEnvironment,
    runEnvironmentLoading,
    runEnvironmentError,
    availableToolchainKinds,
    selectedToolchainOptions,
    missingToolchainKinds,
    customToolchainDrafts,
    envVarsDraft,
    commandPreview,
    envPreview,
    environmentHints,
    envVarsPlaceholder,
    selectedRunTargetId,
    selectRunTarget,
    updateSelectedToolchain,
    updateCustomToolchainDraft,
    saveCustomToolchain,
    setEnvVarsDraft,
    saveEnvVarsDraft,
    setTerminalUiEnabled,
    loadRunCatalog,
    loadRunEnvironment,
    refreshRunnerState,
    invalidateRunnerCatalogState,
  };
};
