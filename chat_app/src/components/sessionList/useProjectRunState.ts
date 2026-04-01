import { useCallback, useEffect, useMemo, useState } from 'react';
import type ApiClient from '../../lib/api/client';
import type { Project, Terminal } from '../../types';
import { normalizeProjectRunCatalog } from '../projectExplorer/utils';

export interface ProjectRunTargetOption {
  id: string;
  label: string;
  cwd: string;
  command: string;
}

export interface ProjectRunViewState {
  status: string;
  loading: boolean;
  targetCount: number;
  defaultTargetId: string | null;
  fallbackTargetId: string | null;
  defaultCommand: string | null;
  defaultCwd: string | null;
  fallbackCommand: string | null;
  fallbackCwd: string | null;
  targets: ProjectRunTargetOption[];
  error: string | null;
}

export interface ProjectLiveViewState {
  isRunning: boolean;
  terminalId: string | null;
  terminalName: string | null;
  canRestart: boolean;
  actionLoading: boolean;
}

interface UseProjectRunStateParams {
  apiClient: ApiClient;
  projects: Project[];
  terminals: Terminal[];
  loadTerminals: () => Promise<unknown>;
  handleSelectTerminal: (terminalId: string) => Promise<void>;
  setActivePanel: (panel: 'chat' | 'project' | 'terminal' | 'remote_terminal' | 'remote_sftp') => void;
}

const createInitialProjectRunState = (): ProjectRunViewState => ({
  status: 'analyzing',
  loading: true,
  targetCount: 0,
  defaultTargetId: null,
  fallbackTargetId: null,
  defaultCommand: null,
  defaultCwd: null,
  fallbackCommand: null,
  fallbackCwd: null,
  targets: [],
  error: null,
});

const resolveProjectRunState = async (
  apiClient: ApiClient,
  projectId: string,
): Promise<ProjectRunViewState> => {
  try {
    const raw = await apiClient.getProjectRunCatalog(projectId);
    const catalog = normalizeProjectRunCatalog(raw);
    const fallbackTarget = catalog.targets[0] || null;
    const defaultTarget = catalog.defaultTargetId
      ? catalog.targets.find((item) => item.id === catalog.defaultTargetId) || null
      : (catalog.targets.find((item) => item.isDefault) || null);
    const targetCount = catalog.targets.length;

    return {
      status: targetCount > 0 ? 'ready' : (catalog.status || 'empty'),
      loading: false,
      targetCount,
      defaultTargetId: catalog.defaultTargetId ? String(catalog.defaultTargetId) : null,
      fallbackTargetId: fallbackTarget?.id ? String(fallbackTarget.id) : null,
      defaultCommand: defaultTarget?.command ? String(defaultTarget.command) : null,
      defaultCwd: defaultTarget?.cwd ? String(defaultTarget.cwd) : null,
      fallbackCommand: fallbackTarget?.command ? String(fallbackTarget.command) : null,
      fallbackCwd: fallbackTarget?.cwd ? String(fallbackTarget.cwd) : null,
      targets: (catalog.targets || []).map((target) => ({
        id: String(target.id || ''),
        label: String(target.label || target.command || '未命名目标'),
        cwd: String(target.cwd || ''),
        command: String(target.command || ''),
      })),
      error: catalog.errorMessage ? String(catalog.errorMessage) : null,
    };
  } catch (error) {
    return {
      ...createInitialProjectRunState(),
      status: 'error',
      loading: false,
      error: error instanceof Error ? error.message : '运行目标加载失败',
    };
  }
};

const resolveProjectRuntimeTerminal = (
  terminals: Terminal[],
  projectId: string,
) => {
  const related = terminals
    .filter((terminal) => String(terminal?.projectId || '') === projectId && terminal?.status === 'running')
    .sort((left, right) => {
      const leftTime = new Date(left?.lastActiveAt || 0).getTime();
      const rightTime = new Date(right?.lastActiveAt || 0).getTime();
      return rightTime - leftTime;
    });
  const busyTerminal = related.find((terminal) => Boolean(terminal?.busy));
  return {
    busyTerminal,
    activeTerminal: busyTerminal || related[0] || null,
  };
};

export const useProjectRunState = ({
  apiClient,
  projects,
  terminals,
  loadTerminals,
  handleSelectTerminal,
  setActivePanel,
}: UseProjectRunStateParams) => {
  const [projectRunStateById, setProjectRunStateById] = useState<Record<string, ProjectRunViewState>>({});
  const [runningProjectId, setRunningProjectId] = useState<string | null>(null);
  const [projectActionLoadingById, setProjectActionLoadingById] = useState<Record<string, boolean>>({});

  useEffect(() => {
    let cancelled = false;
    const projectIds = new Set((projects || []).map((project) => String(project.id || '')));

    setProjectRunStateById((prev) => {
      const next: Record<string, ProjectRunViewState> = {};
      (projects || []).forEach((project) => {
        next[project.id] = prev[project.id] || createInitialProjectRunState();
      });
      return next;
    });

    const loadProjectRunStates = async () => {
      const updates = await Promise.all(
        (projects || []).map(async (project) => ({
          projectId: project.id,
          state: await resolveProjectRunState(apiClient, project.id),
        })),
      );

      if (cancelled) {
        return;
      }

      setProjectRunStateById((prev) => {
        const next: Record<string, ProjectRunViewState> = {};
        projectIds.forEach((projectId) => {
          if (prev[projectId]) {
            next[projectId] = prev[projectId];
          }
        });
        updates.forEach((item) => {
          next[item.projectId] = item.state;
        });
        return next;
      });
    };

    void loadProjectRunStates();

    return () => {
      cancelled = true;
    };
  }, [apiClient, projects]);

  const projectLiveStateById = useMemo<Record<string, ProjectLiveViewState>>(() => {
    const out: Record<string, ProjectLiveViewState> = {};

    (projects || []).forEach((project) => {
      const { busyTerminal, activeTerminal } = resolveProjectRuntimeTerminal(terminals || [], project.id);
      const runState = projectRunStateById[project.id];
      const command = runState?.defaultCommand || runState?.fallbackCommand || null;
      const cwd = runState?.defaultCwd || runState?.fallbackCwd || activeTerminal?.cwd || null;

      out[project.id] = {
        isRunning: Boolean(busyTerminal),
        terminalId: activeTerminal?.id || null,
        terminalName: activeTerminal?.name || null,
        canRestart: Boolean(command && cwd),
        actionLoading: Boolean(projectActionLoadingById[project.id]),
      };
    });

    return out;
  }, [projectActionLoadingById, projectRunStateById, projects, terminals]);

  const setProjectRunError = useCallback((projectId: string, error: string | null) => {
    setProjectRunStateById((prev) => {
      const current = prev[projectId];
      if (!current) {
        return prev;
      }
      return {
        ...prev,
        [projectId]: {
          ...current,
          error,
        },
      };
    });
  }, []);

  const setProjectActionLoading = useCallback((projectId: string, loading: boolean) => {
    setProjectActionLoadingById((prev) => ({
      ...prev,
      [projectId]: loading,
    }));
  }, []);

  const handleRunProject = useCallback(async (projectId: string, chosenTargetId?: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    const runState = projectRunStateById[projectId];
    if (!project || !runState) {
      return;
    }

    const targetId = (chosenTargetId || '').trim() || runState.defaultTargetId || runState.fallbackTargetId;
    if (!targetId) {
      return;
    }

    setRunningProjectId(projectId);
    setProjectActionLoading(projectId, true);
    setProjectRunError(projectId, null);

    try {
      const result = await apiClient.executeProjectRun(projectId, {
        target_id: targetId,
        create_if_missing: true,
      });
      const terminalId = String((result as { terminal_id?: string | null })?.terminal_id || '').trim();
      if (terminalId) {
        await handleSelectTerminal(terminalId);
      }
      setActivePanel('terminal');
      await loadTerminals();
    } catch (error) {
      setProjectRunError(projectId, error instanceof Error ? error.message : '运行失败');
    } finally {
      setProjectActionLoading(projectId, false);
      setRunningProjectId((prev) => (prev === projectId ? null : prev));
    }
  }, [apiClient, handleSelectTerminal, loadTerminals, projectRunStateById, projects, setActivePanel, setProjectActionLoading, setProjectRunError]);

  const handleStopProject = useCallback(async (projectId: string) => {
    const live = projectLiveStateById[projectId];
    if (!live?.terminalId) {
      return;
    }

    setProjectActionLoading(projectId, true);
    setProjectRunError(projectId, null);

    try {
      await apiClient.interruptTerminal(live.terminalId, { reason: 'project_list_stop' });
      await loadTerminals();
      setActivePanel('terminal');
    } catch (error) {
      setProjectRunError(projectId, error instanceof Error ? error.message : '停止失败');
    } finally {
      setProjectActionLoading(projectId, false);
    }
  }, [apiClient, loadTerminals, projectLiveStateById, setActivePanel, setProjectActionLoading, setProjectRunError]);

  const handleRestartProject = useCallback(async (projectId: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    const live = projectLiveStateById[projectId];
    const runState = projectRunStateById[projectId];
    const command = runState?.defaultCommand || runState?.fallbackCommand;
    const fallbackTerminalCwd = (terminals || []).find((terminal) => terminal.id === live?.terminalId)?.cwd || '';
    const cwd = (runState?.defaultCwd || runState?.fallbackCwd || fallbackTerminalCwd || '').trim();

    if (!project) {
      return;
    }
    if (!command || !cwd) {
      setProjectRunError(projectId, '未找到可重启命令，请先重扫目标');
      return;
    }

    setProjectActionLoading(projectId, true);
    setProjectRunError(projectId, null);

    try {
      if (live?.terminalId && live.isRunning) {
        await apiClient.interruptTerminal(live.terminalId, { reason: 'project_list_restart' });
        await new Promise((resolve) => setTimeout(resolve, 180));
      }
      const result = await apiClient.dispatchTerminalCommand({
        cwd,
        command,
        project_id: project.id,
        create_if_missing: true,
      });
      const terminalId = String((result as { terminal_id?: string | null })?.terminal_id || '').trim();
      if (terminalId) {
        await handleSelectTerminal(terminalId);
      }
      setActivePanel('terminal');
      await loadTerminals();
    } catch (error) {
      setProjectRunError(projectId, error instanceof Error ? error.message : '重启失败');
    } finally {
      setProjectActionLoading(projectId, false);
    }
  }, [apiClient, handleSelectTerminal, loadTerminals, projectLiveStateById, projectRunStateById, projects, setActivePanel, setProjectActionLoading, setProjectRunError, terminals]);

  return {
    projectRunStateById,
    projectLiveStateById,
    runningProjectId,
    handleRunProject,
    handleStopProject,
    handleRestartProject,
  };
};
