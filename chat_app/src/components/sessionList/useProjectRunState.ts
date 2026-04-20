import { useCallback, useEffect, useMemo, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type { Project, Terminal } from '../../types';
import { normalizeEntry } from '../projectExplorer/utils';

const RUNNER_SCRIPT_DIR = '.chatos';
const RUNNER_SCRIPT_FILE = 'project_runner.sh';
const RUNNER_SCRIPT_REL_PATH = `${RUNNER_SCRIPT_DIR}/${RUNNER_SCRIPT_FILE}`;
const RUNNER_START_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} start`;
const RUNNER_STOP_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} stop`;
const RUNNER_RESTART_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} restart`;

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
  status: 'loading',
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

const normalizeRootPath = (value: string): string => value.trim().replace(/[\\/]+$/, '');

const hasRunnerScript = async (apiClient: ApiClient, rootPath: string): Promise<boolean> => {
  const safeRoot = normalizeRootPath(rootPath);
  if (!safeRoot) {
    return false;
  }
  const rootList = await apiClient.listFsEntries(safeRoot);
  const rootEntries = Array.isArray(rootList?.entries) ? rootList.entries.map(normalizeEntry) : [];
  const runnerDirEntry = rootEntries.find((entry) => entry.isDir && entry.name === RUNNER_SCRIPT_DIR) || null;
  const runnerDirPath = runnerDirEntry?.path || `${safeRoot}/${RUNNER_SCRIPT_DIR}`;
  try {
    const runnerList = await apiClient.listFsEntries(runnerDirPath);
    const runnerEntries = Array.isArray(runnerList?.entries) ? runnerList.entries.map(normalizeEntry) : [];
    return runnerEntries.some((entry) => !entry.isDir && entry.name === RUNNER_SCRIPT_FILE);
  } catch {
    return false;
  }
};

const resolveProjectRunState = async (
  apiClient: ApiClient,
  project: Project,
): Promise<ProjectRunViewState> => {
  try {
    const [members, scriptExists] = await Promise.all([
      apiClient.listProjectContacts(project.id, { limit: 500, offset: 0 }),
      hasRunnerScript(apiClient, project.rootPath),
    ]);
    const memberCount = Array.isArray(members) ? members.length : 0;
    if (scriptExists) {
      const target: ProjectRunTargetOption = {
        id: 'project_runner_start',
        label: 'project_runner.sh start',
        cwd: project.rootPath,
        command: RUNNER_START_COMMAND,
      };
      return {
        status: 'ready',
        loading: false,
        targetCount: 1,
        defaultTargetId: target.id,
        fallbackTargetId: target.id,
        defaultCommand: target.command,
        defaultCwd: target.cwd,
        fallbackCommand: target.command,
        fallbackCwd: target.cwd,
        targets: [target],
        error: null,
      };
    }
    if (memberCount <= 0) {
      return {
        ...createInitialProjectRunState(),
        status: 'no_member',
        loading: false,
        error: '请先添加一个联系人',
      };
    }
    return {
      ...createInitialProjectRunState(),
      status: 'script_missing',
      loading: false,
      defaultCwd: project.rootPath,
      fallbackCwd: project.rootPath,
      error: '请先在项目页生成启动脚本',
    };
  } catch (error) {
    return {
      ...createInitialProjectRunState(),
      status: 'error',
      loading: false,
      error: error instanceof Error ? error.message : '运行状态加载失败',
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
    let timer: ReturnType<typeof setTimeout> | null = null;
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
          state: await resolveProjectRunState(apiClient, project),
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

      if (!cancelled && updates.some((item) => item.state.status !== 'ready')) {
        timer = setTimeout(() => {
          void loadProjectRunStates();
        }, 5000);
      }
    };

    void loadProjectRunStates();

    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
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

  const handleRunProject = useCallback(async (projectId: string, _chosenTargetId?: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    const runState = projectRunStateById[projectId];
    const cwd = (runState?.defaultCwd || runState?.fallbackCwd || project?.rootPath || '').trim();
    if (!project || !runState) {
      return;
    }
    if (!cwd || runState.status !== 'ready') {
      setProjectRunError(projectId, runState.error || '请先在项目页生成启动脚本');
      return;
    }

    setRunningProjectId(projectId);
    setProjectActionLoading(projectId, true);
    setProjectRunError(projectId, null);

    try {
      const result = await apiClient.dispatchTerminalCommand({
        cwd,
        command: RUNNER_START_COMMAND,
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
      setProjectRunError(projectId, error instanceof Error ? error.message : '运行失败');
    } finally {
      setProjectActionLoading(projectId, false);
      setRunningProjectId((prev) => (prev === projectId ? null : prev));
    }
  }, [apiClient, handleSelectTerminal, loadTerminals, projectRunStateById, projects, setActivePanel, setProjectActionLoading, setProjectRunError]);

  const handleStopProject = useCallback(async (projectId: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    const runState = projectRunStateById[projectId];
    const cwd = (runState?.defaultCwd || runState?.fallbackCwd || project?.rootPath || '').trim();
    if (!project || !cwd) {
      return;
    }

    setProjectActionLoading(projectId, true);
    setProjectRunError(projectId, null);

    try {
      const result = await apiClient.dispatchTerminalCommand({
        cwd,
        command: RUNNER_STOP_COMMAND,
        project_id: project.id,
        create_if_missing: true,
      });
      const terminalId = String((result as { terminal_id?: string | null })?.terminal_id || '').trim();
      if (terminalId) {
        await handleSelectTerminal(terminalId);
      }
      await loadTerminals();
      setActivePanel('terminal');
    } catch (error) {
      setProjectRunError(projectId, error instanceof Error ? error.message : '停止失败');
    } finally {
      setProjectActionLoading(projectId, false);
    }
  }, [apiClient, handleSelectTerminal, loadTerminals, projectRunStateById, projects, setActivePanel, setProjectActionLoading, setProjectRunError]);

  const handleRestartProject = useCallback(async (projectId: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    const runState = projectRunStateById[projectId];
    const cwd = (runState?.defaultCwd || runState?.fallbackCwd || project?.rootPath || '').trim();

    if (!project || !cwd) {
      return;
    }

    setProjectActionLoading(projectId, true);
    setProjectRunError(projectId, null);

    try {
      const result = await apiClient.dispatchTerminalCommand({
        cwd,
        command: RUNNER_RESTART_COMMAND,
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
  }, [apiClient, handleSelectTerminal, loadTerminals, projectRunStateById, projects, setActivePanel, setProjectActionLoading, setProjectRunError]);

  return {
    projectRunStateById,
    projectLiveStateById,
    runningProjectId,
    handleRunProject,
    handleStopProject,
    handleRestartProject,
  };
};
