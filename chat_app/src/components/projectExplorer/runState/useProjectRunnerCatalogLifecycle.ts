import { useCallback, useEffect, useRef } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import type ApiClient from '../../../lib/api/client';
import {
  normalizeProjectRunCatalog,
  normalizeProjectRunEnvironment,
} from '../../../lib/domain/projectExplorer';
import { useProjectRunRealtime } from '../../../lib/realtime/useProjectRunRealtime';
import type { RealtimeProjectRunCatalogPayloadWrapper } from '../../../lib/realtime/types';
import type {
  Project,
  ProjectRunEnvironment,
  ProjectRunTarget,
} from '../../../types';
import {
  mergeProjectRunRefreshMode,
  resolveProjectRunnerRealtimeCatalogAction,
  type ProjectRunRefreshMode,
} from './projectRunnerCatalogState';
import { serializeEnvVarsDraft } from './projectRunnerEnvironmentState';
import { createProjectRunnerRequestGuard } from './projectRunnerRequestGuard';

interface UseProjectRunnerCatalogLifecycleOptions {
  client: ApiClient;
  project: Project | null;
  enabled: boolean;
  setRunTargets: Dispatch<SetStateAction<ProjectRunTarget[]>>;
  setRunCatalogLoading: Dispatch<SetStateAction<boolean>>;
  setRunCatalogError: Dispatch<SetStateAction<string | null>>;
  setRunEnvironment: Dispatch<SetStateAction<ProjectRunEnvironment | null>>;
  setRunEnvironmentLoading: Dispatch<SetStateAction<boolean>>;
  setRunEnvironmentError: Dispatch<SetStateAction<string | null>>;
  setSelectedRunTargetId: Dispatch<SetStateAction<string | null>>;
  setCustomToolchainDrafts: Dispatch<SetStateAction<Record<string, string>>>;
  setEnvVarsDraft: Dispatch<SetStateAction<string>>;
  applyRunCatalog: (catalog: ReturnType<typeof normalizeProjectRunCatalog>) => void;
}

export const useProjectRunnerCatalogLifecycle = ({
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
}: UseProjectRunnerCatalogLifecycleOptions) => {
  const runCatalogRequestRef = useRef<Promise<void> | null>(null);
  const queuedRunCatalogModeRef = useRef<ProjectRunRefreshMode | null>(null);
  const runCatalogVersionRef = useRef(0);
  const runEnvironmentVersionRef = useRef(0);
  const activeProjectKeyRef = useRef<string | null>(null);

  const resetProjectRunnerCatalogState = useCallback(() => {
    queuedRunCatalogModeRef.current = null;
    runCatalogRequestRef.current = null;
    runCatalogVersionRef.current += 1;
    runEnvironmentVersionRef.current += 1;
    setRunTargets([]);
    setRunCatalogError(null);
    setSelectedRunTargetId(null);
    setRunCatalogLoading(false);
    setRunEnvironment(null);
    setRunEnvironmentLoading(false);
    setRunEnvironmentError(null);
    setCustomToolchainDrafts({});
    setEnvVarsDraft('');
  }, [
    setCustomToolchainDrafts,
    setEnvVarsDraft,
    setRunCatalogError,
    setRunCatalogLoading,
    setRunEnvironment,
    setRunEnvironmentError,
    setRunEnvironmentLoading,
    setRunTargets,
    setSelectedRunTargetId,
  ]);

  const loadRunEnvironment = useCallback(async () => {
    if (!enabled || !project?.id) {
      setRunEnvironment(null);
      setRunEnvironmentLoading(false);
      setRunEnvironmentError(null);
      return;
    }

    const projectId = project.id;
    const guard = createProjectRunnerRequestGuard({
      enabled,
      projectId,
      versionRef: runEnvironmentVersionRef,
    });
    if (!guard) {
      return;
    }
    setRunEnvironmentLoading(true);
    setRunEnvironmentError(null);
    try {
      const raw = await client.getProjectRunEnvironment(projectId);
      if (!guard.shouldApply()) {
        return;
      }
      const normalized = normalizeProjectRunEnvironment(raw);
      setRunEnvironment(normalized);
      setEnvVarsDraft(serializeEnvVarsDraft(normalized.envVars));
    } catch (error) {
      if (!guard.shouldApply()) {
        return;
      }
      setRunEnvironment(null);
      setRunEnvironmentError(error instanceof Error ? error.message : '加载运行环境失败');
    } finally {
      if (guard.shouldApply()) {
        setRunEnvironmentLoading(false);
      }
    }
  }, [client, enabled, project?.id, setEnvVarsDraft, setRunEnvironment, setRunEnvironmentError, setRunEnvironmentLoading]);

  const loadRunCatalogOnce = useCallback(async (mode: ProjectRunRefreshMode = 'analyze') => {
    if (!enabled || !project?.id) {
      setRunTargets([]);
      setRunCatalogLoading(false);
      setRunCatalogError(null);
      setSelectedRunTargetId(null);
      return;
    }

    const projectId = project.id;
    const guard = createProjectRunnerRequestGuard({
      enabled,
      projectId,
      versionRef: runCatalogVersionRef,
    });
    if (!guard) {
      return;
    }
    setRunCatalogLoading(true);
    setRunCatalogError(null);
    try {
      const raw = mode === 'catalog'
        ? await client.getProjectRunCatalog(projectId)
        : await client.analyzeProjectRun(projectId);
      if (!guard.shouldApply()) {
        return;
      }
      const catalog = normalizeProjectRunCatalog(raw);
      applyRunCatalog(catalog);
    } catch (error) {
      if (!guard.shouldApply()) {
        return;
      }
      setRunTargets([]);
      setRunCatalogError(error instanceof Error ? error.message : '分析运行目标失败');
      setSelectedRunTargetId(null);
    } finally {
      if (guard.shouldApply()) {
        setRunCatalogLoading(false);
      }
    }
  }, [
    applyRunCatalog,
    client,
    enabled,
    project?.id,
    setRunCatalogError,
    setRunCatalogLoading,
    setRunTargets,
    setSelectedRunTargetId,
  ]);

  const queueRunCatalogMode = useCallback((mode: ProjectRunRefreshMode) => {
    queuedRunCatalogModeRef.current = mergeProjectRunRefreshMode(
      queuedRunCatalogModeRef.current,
      mode,
    );
  }, []);

  const loadRunCatalog = useCallback(async (mode: ProjectRunRefreshMode = 'analyze') => {
    if (!enabled || !project?.id) {
      setRunTargets([]);
      setRunCatalogLoading(false);
      setRunCatalogError(null);
      setSelectedRunTargetId(null);
      return;
    }

    if (runCatalogRequestRef.current) {
      queueRunCatalogMode(mode);
      await runCatalogRequestRef.current;
      return;
    }

    const run = async () => {
      let nextMode: ProjectRunRefreshMode | null = mode;
      do {
        const activeMode = nextMode || 'catalog';
        queuedRunCatalogModeRef.current = null;
        nextMode = null;
        await loadRunCatalogOnce(activeMode);
        nextMode = queuedRunCatalogModeRef.current;
      } while (nextMode && enabled && Boolean(project?.id));
    };

    const request = run().finally(() => {
      runCatalogRequestRef.current = null;
      queuedRunCatalogModeRef.current = null;
    });
    runCatalogRequestRef.current = request;
    await request;
  }, [enabled, loadRunCatalogOnce, project?.id, queueRunCatalogMode, setRunCatalogError, setRunCatalogLoading, setRunTargets, setSelectedRunTargetId]);

  const selectRunTarget = useCallback(async (targetId: string) => {
    const normalizedTargetId = targetId.trim();
    if (!enabled || !project?.id || !normalizedTargetId) {
      return;
    }

    setSelectedRunTargetId(normalizedTargetId);
    try {
      const raw = await client.setProjectRunDefault(project.id, normalizedTargetId);
      const catalog = normalizeProjectRunCatalog(raw);
      applyRunCatalog(catalog);
      setSelectedRunTargetId(catalog.defaultTargetId || normalizedTargetId);
    } catch (error) {
      setRunCatalogError(error instanceof Error ? error.message : '设置默认运行目标失败');
    }
  }, [applyRunCatalog, client, enabled, project?.id, setRunCatalogError, setSelectedRunTargetId]);

  const refreshRunnerState = useCallback(async (mode: ProjectRunRefreshMode = 'catalog') => {
    await Promise.all([
      loadRunCatalog(mode),
      loadRunEnvironment(),
    ]);
  }, [loadRunCatalog, loadRunEnvironment]);

  const invalidateRunnerCatalogState = useCallback(() => {
    resetProjectRunnerCatalogState();
  }, [resetProjectRunnerCatalogState]);

  useEffect(() => {
    const nextProjectKey = enabled ? project?.id || null : null;
    if (activeProjectKeyRef.current === nextProjectKey) {
      return;
    }

    activeProjectKeyRef.current = nextProjectKey;
    invalidateRunnerCatalogState();
    if (nextProjectKey) {
      void refreshRunnerState('catalog');
    }
  }, [enabled, invalidateRunnerCatalogState, project?.id, refreshRunnerState]);

  useProjectRunRealtime({
    enabled: enabled && Boolean(project?.id),
    projectId: project?.id || null,
    onCatalogUpdated: async (payload: RealtimeProjectRunCatalogPayloadWrapper) => {
      const action = resolveProjectRunnerRealtimeCatalogAction(payload);
      if (action === 'reset') {
        invalidateRunnerCatalogState();
        return;
      }
      if (action === 'reload_environment') {
        await loadRunEnvironment();
        return;
      }
      await loadRunCatalog('catalog');
    },
  });

  return {
    loadRunCatalog,
    loadRunEnvironment,
    refreshRunnerState,
    invalidateRunnerCatalogState,
    selectRunTarget,
  };
};
