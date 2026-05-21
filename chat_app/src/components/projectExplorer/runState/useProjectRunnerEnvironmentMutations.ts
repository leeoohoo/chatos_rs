import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import type ApiClient from '../../../lib/api/client';
import { normalizeProjectRunEnvironment } from '../../../lib/domain/projectExplorer';
import type {
  Project,
  ProjectRunCustomToolchain,
  ProjectRunEnvironment,
} from '../../../types';
import { serializeEnvVarsDraft } from './projectRunnerEnvironmentState';
import {
  buildProjectRunEnvironmentUpdatePayload,
  resolveCustomToolchainEnvironment,
  resolveEnvVarsEnvironment,
  resolveSelectedToolchainEnvironment,
} from './projectRunnerEnvironmentActions';

interface UseProjectRunnerEnvironmentMutationsOptions {
  client: ApiClient;
  project: Project | null;
  enabled: boolean;
  runEnvironment: ProjectRunEnvironment | null;
  customToolchainDrafts: Record<string, string>;
  envVarsDraft: string;
  setRunEnvironment: Dispatch<SetStateAction<ProjectRunEnvironment | null>>;
  setRunEnvironmentError: Dispatch<SetStateAction<string | null>>;
  setEnvVarsDraft: Dispatch<SetStateAction<string>>;
  loadRunEnvironment: () => Promise<void>;
}

export const useProjectRunnerEnvironmentMutations = ({
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
}: UseProjectRunnerEnvironmentMutationsOptions) => {
  const persistEnvironment = useCallback(async (
    nextSelectedToolchains: Record<string, string>,
    nextCustomToolchains: Record<string, ProjectRunCustomToolchain>,
    nextEnvVars: Record<string, string>,
  ) => {
    if (!enabled || !project?.id) {
      return;
    }
    const raw = await client.updateProjectRunEnvironment(project.id, {
      ...buildProjectRunEnvironmentUpdatePayload({
        selectedToolchains: nextSelectedToolchains,
        customToolchains: nextCustomToolchains,
        envVars: nextEnvVars,
      }),
    });
    const normalized = normalizeProjectRunEnvironment(raw);
    setRunEnvironment(normalized);
    setEnvVarsDraft(serializeEnvVarsDraft(normalized.envVars));
    setRunEnvironmentError(null);
  }, [client, enabled, project?.id, setEnvVarsDraft, setRunEnvironment, setRunEnvironmentError]);

  const updateSelectedToolchain = useCallback(async (kind: string, optionId: string) => {
    const normalizedKind = kind.trim();
    const normalizedOptionId = optionId.trim();
    if (!enabled || !project?.id || !normalizedKind || !normalizedOptionId) {
      return;
    }

    const nextSelectedToolchains = {
      ...(runEnvironment?.selectedToolchains || {}),
      [normalizedKind]: normalizedOptionId,
    };

    setRunEnvironment((prev) => resolveSelectedToolchainEnvironment({
      environment: prev,
      kind: normalizedKind,
      optionId: normalizedOptionId,
    }));

    try {
      await persistEnvironment(
        nextSelectedToolchains,
        runEnvironment?.customToolchains || {},
        runEnvironment?.envVars || {},
      );
    } catch (error) {
      setRunEnvironmentError(error instanceof Error ? error.message : '更新运行环境失败');
      await loadRunEnvironment();
    }
  }, [
    enabled,
    loadRunEnvironment,
    persistEnvironment,
    project?.id,
    runEnvironment?.customToolchains,
    runEnvironment?.envVars,
    runEnvironment?.selectedToolchains,
    setRunEnvironment,
    setRunEnvironmentError,
  ]);

  const saveCustomToolchain = useCallback(async (kind: string) => {
    const normalizedKind = kind.trim();
    const draftPath = (customToolchainDrafts[normalizedKind] || '').trim();
    if (!enabled || !project?.id || !normalizedKind || !draftPath) {
      return;
    }

    const nextSelection = resolveCustomToolchainEnvironment({
      environment: runEnvironment,
      kind: normalizedKind,
      draftPath,
    });

    setRunEnvironment(nextSelection.nextEnvironment);

    try {
      await persistEnvironment(
        nextSelection.nextSelectedToolchains,
        nextSelection.nextCustomToolchains,
        runEnvironment?.envVars || {},
      );
    } catch (error) {
      setRunEnvironmentError(error instanceof Error ? error.message : '保存自定义工具链失败');
      await loadRunEnvironment();
    }
  }, [
    customToolchainDrafts,
    enabled,
    loadRunEnvironment,
    persistEnvironment,
    project?.id,
    runEnvironment,
    setRunEnvironment,
    setRunEnvironmentError,
  ]);

  const saveEnvVarsDraft = useCallback(async () => {
    if (!enabled || !project?.id) {
      return;
    }

    const nextEnvState = resolveEnvVarsEnvironment({
      environment: runEnvironment,
      envVarsDraft,
    });
    setRunEnvironment(nextEnvState.nextEnvironment);

    try {
      await persistEnvironment(
        runEnvironment?.selectedToolchains || {},
        runEnvironment?.customToolchains || {},
        nextEnvState.nextEnvVars,
      );
    } catch (error) {
      setRunEnvironmentError(error instanceof Error ? error.message : '保存环境变量失败');
      await loadRunEnvironment();
    }
  }, [
    enabled,
    envVarsDraft,
    loadRunEnvironment,
    persistEnvironment,
    project?.id,
    runEnvironment,
    setRunEnvironment,
    setRunEnvironmentError,
  ]);

  return {
    persistEnvironment,
    updateSelectedToolchain,
    saveCustomToolchain,
    saveEnvVarsDraft,
  };
};
