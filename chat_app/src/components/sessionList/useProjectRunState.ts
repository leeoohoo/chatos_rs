import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { useRealtimeEvent, useRealtimeTopics } from '../../lib/realtime/RealtimeProvider';
import { getRealtimeConnectionStateSnapshot } from '../../lib/realtime/state';
import type {
  RealtimeEventEnvelope,
  RealtimeProjectMembersUpdatedPayloadWrapper,
  RealtimeProjectRunCatalogPayloadWrapper,
  RealtimeProjectRunStatePayloadWrapper,
} from '../../lib/realtime/types';
import type { Project, Terminal } from '../../types';
import {
  RUNNER_RESTART_COMMAND,
  RUNNER_START_COMMAND,
  RUNNER_STOP_COMMAND,
  buildProjectRunnerTarget,
  hasProjectRunnerScript,
  isProjectRunnerPathMissingError,
  loadProjectRunnerContactRows,
  markProjectRunnerContactRowsStale,
  markProjectRunnerScriptStateStale,
  normalizeProjectRunnerRootPath,
  patchProjectRunnerScriptStateSnapshot,
  readProjectRunnerErrorMessage,
  resolveProjectRuntimeTerminal,
} from '../../lib/domain/projectRunner';

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
  enabled?: boolean;
}

interface ProjectRealtimeLiveState {
  isRunning: boolean;
  terminalId: string | null;
  terminalName: string | null;
  cwd: string | null;
  busy: boolean;
  status: string;
}

interface ProjectRunStateDetails {
  memberCount: number;
  runnerScriptExists: boolean;
  runnerRootMissing: boolean;
  membersError: string | null;
  runnerScriptError: string | null;
  membersLoading: boolean;
  runnerScriptLoading: boolean;
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

const createInitialProjectRunStateDetails = (): ProjectRunStateDetails => ({
  memberCount: 0,
  runnerScriptExists: false,
  runnerRootMissing: false,
  membersError: null,
  runnerScriptError: null,
  membersLoading: true,
  runnerScriptLoading: true,
});

const isProjectRunStatePayload = (
  event: RealtimeEventEnvelope,
): event is RealtimeEventEnvelope & { payload: RealtimeProjectRunStatePayloadWrapper } => (
  event?.payload?.kind === 'project_run_state'
);

const isProjectRunCatalogPayload = (
  event: RealtimeEventEnvelope,
): event is RealtimeEventEnvelope & { payload: RealtimeProjectRunCatalogPayloadWrapper } => (
  event?.payload?.kind === 'project_run_catalog'
);

const isProjectMembersUpdatedPayload = (
  event: RealtimeEventEnvelope,
): event is RealtimeEventEnvelope & { payload: RealtimeProjectMembersUpdatedPayloadWrapper } => (
  event?.payload?.kind === 'project_members_updated'
);

const readTrimmedString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const shouldFallbackRefreshTerminals = (): boolean => (
  getRealtimeConnectionStateSnapshot() !== 'connected'
);

const buildProjectRunViewState = (
  project: Project,
  details: ProjectRunStateDetails,
): ProjectRunViewState => {
  const loading = details.membersLoading || details.runnerScriptLoading;

  if (details.runnerRootMissing) {
    return {
      ...createInitialProjectRunState(),
      status: 'missing_root',
      loading: false,
      error: '项目目录不存在，请检查项目路径',
    };
  }

  if (loading) {
    return {
      ...createInitialProjectRunState(),
      status: 'loading',
      loading: true,
    };
  }

  if (details.membersError || details.runnerScriptError) {
    return {
      ...createInitialProjectRunState(),
      status: 'error',
      loading: false,
      error: details.runnerScriptError || details.membersError || '运行状态加载失败',
    };
  }

  if (details.runnerScriptExists) {
    const target = buildProjectRunnerTarget(project.rootPath);
    return {
      ...createInitialProjectRunState(),
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

  if (details.memberCount <= 0) {
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
};

const loadProjectRunMembersDetails = async (
  apiClient: ApiClient,
  projectId: string,
): Promise<Partial<ProjectRunStateDetails>> => {
  try {
    const members = await loadProjectRunnerContactRows(apiClient, projectId);
    return {
      memberCount: Array.isArray(members) ? members.length : 0,
      membersError: null,
      membersLoading: false,
    };
  } catch (error) {
    return {
      memberCount: 0,
      membersError: readProjectRunnerErrorMessage(error, '运行状态加载失败'),
      membersLoading: false,
    };
  }
};

const loadProjectRunScriptDetails = async (
  apiClient: ApiClient,
  project: Project,
): Promise<Partial<ProjectRunStateDetails>> => {
  try {
    const scriptExists = await hasProjectRunnerScript(apiClient, project.rootPath);
    return {
      runnerScriptExists: scriptExists,
      runnerRootMissing: false,
      runnerScriptError: null,
      runnerScriptLoading: false,
    };
  } catch (error) {
    if (isProjectRunnerPathMissingError(error)) {
      return {
        runnerScriptExists: false,
        runnerRootMissing: true,
        runnerScriptError: '项目目录不存在，请检查项目路径',
        runnerScriptLoading: false,
      };
    }

    return {
      runnerScriptExists: false,
      runnerRootMissing: false,
      runnerScriptError: readProjectRunnerErrorMessage(error, '运行状态加载失败'),
      runnerScriptLoading: false,
    };
  }
};

const resolveProjectRunScriptSnapshotDetails = (
  apiClient: ApiClient,
  project: Project,
  payload: RealtimeProjectRunCatalogPayloadWrapper,
): Partial<ProjectRunStateDetails> | null => {
  const hasRootMissing = typeof payload.root_missing === 'boolean';
  const hasRunnerScriptExists = typeof payload.runner_script_exists === 'boolean';
  if (!hasRootMissing && !hasRunnerScriptExists) {
    return null;
  }

  if (hasRunnerScriptExists && project.rootPath) {
    patchProjectRunnerScriptStateSnapshot(
      apiClient,
      project.rootPath,
      payload.runner_script_exists === true,
    );
  }

  const next: Partial<ProjectRunStateDetails> = {
    runnerScriptLoading: false,
  };

  if (hasRunnerScriptExists) {
    next.runnerScriptExists = payload.runner_script_exists === true;
    next.runnerRootMissing = false;
    next.runnerScriptError = null;
  }

  if (hasRootMissing) {
    next.runnerRootMissing = payload.root_missing === true;
    next.runnerScriptError = payload.root_missing === true ? '项目目录不存在，请检查项目路径' : null;
    if (payload.root_missing === true) {
      next.runnerScriptExists = false;
    }
  }

  return next;
};

const resolveProjectRunState = async (
  apiClient: ApiClient,
  project: Project,
): Promise<{
  details: ProjectRunStateDetails;
  viewState: ProjectRunViewState;
}> => {
  const [memberDetails, scriptDetails] = await Promise.all([
    loadProjectRunMembersDetails(apiClient, project.id),
    loadProjectRunScriptDetails(apiClient, project),
  ]);
  const details: ProjectRunStateDetails = {
    ...createInitialProjectRunStateDetails(),
    ...memberDetails,
    ...scriptDetails,
  };
  return {
    details,
    viewState: buildProjectRunViewState(project, details),
  };
};

export const useProjectRunState = ({
  apiClient,
  projects,
  terminals,
  loadTerminals,
  handleSelectTerminal,
  setActivePanel,
  enabled = true,
}: UseProjectRunStateParams) => {
  const [projectRunStateById, setProjectRunStateById] = useState<Record<string, ProjectRunViewState>>({});
  const [projectRealtimeLiveById, setProjectRealtimeLiveById] = useState<Record<string, ProjectRealtimeLiveState>>({});
  const [runningProjectId, setRunningProjectId] = useState<string | null>(null);
  const [projectActionLoadingById, setProjectActionLoadingById] = useState<Record<string, boolean>>({});
  const projectRunStateRef = useRef<Record<string, ProjectRunViewState>>({});
  const projectRunStateDetailsRef = useRef<Record<string, ProjectRunStateDetails>>({});
  const projectRootPathByIdRef = useRef<Record<string, string>>({});
  const projectIds = useMemo(
    () => new Set((projects || []).map((project) => String(project.id || '')).filter(Boolean)),
    [projects],
  );
  const realtimeProjectTopics = useMemo(
    () => (projects || []).map((project) => (
      project?.id ? { scope: 'project' as const, id: project.id } : null
    )),
    [projects],
  );

  useRealtimeTopics(realtimeProjectTopics, enabled);

  useEffect(() => {
    if (!enabled) {
      setProjectRunStateById({});
      setProjectRealtimeLiveById({});
      projectRunStateRef.current = {};
      projectRunStateDetailsRef.current = {};
      projectRootPathByIdRef.current = {};
      return;
    }

    let cancelled = false;
    const projectIds = new Set((projects || []).map((project) => String(project.id || '')));

    setProjectRunStateById((prev) => {
      const next: Record<string, ProjectRunViewState> = {};
      const nextDetails: Record<string, ProjectRunStateDetails> = {};
      const nextRootPathById: Record<string, string> = {};
      (projects || []).forEach((project) => {
        const normalizedRootPath = normalizeProjectRunnerRootPath(project.rootPath || '');
        nextRootPathById[project.id] = normalizedRootPath;
        const previousRootPath = projectRootPathByIdRef.current[project.id] || '';
        const currentDetails = previousRootPath === normalizedRootPath
          ? (projectRunStateDetailsRef.current[project.id] || createInitialProjectRunStateDetails())
          : createInitialProjectRunStateDetails();
        nextDetails[project.id] = currentDetails;
        next[project.id] = previousRootPath === normalizedRootPath
          ? (prev[project.id] || createInitialProjectRunState())
          : createInitialProjectRunState();
      });
      projectRunStateRef.current = next;
      projectRunStateDetailsRef.current = nextDetails;
      projectRootPathByIdRef.current = nextRootPathById;
      return next;
    });

    const loadProjectRunStates = async () => {
      const updates = await Promise.all(
        (projects || []).map(async (project) => ({
          projectId: project.id,
          resolved: await resolveProjectRunState(apiClient, project),
        })),
      );

      if (cancelled) {
        return;
      }

      setProjectRunStateById((prev) => {
        const next: Record<string, ProjectRunViewState> = {};
        const nextDetails: Record<string, ProjectRunStateDetails> = {};
        projectIds.forEach((projectId) => {
          if (prev[projectId]) {
            next[projectId] = prev[projectId];
          }
          if (projectRunStateDetailsRef.current[projectId]) {
            nextDetails[projectId] = projectRunStateDetailsRef.current[projectId];
          }
        });
        updates.forEach((item) => {
          next[item.projectId] = item.resolved.viewState;
          nextDetails[item.projectId] = item.resolved.details;
        });
        projectRunStateRef.current = next;
        projectRunStateDetailsRef.current = nextDetails;
        return next;
      });
    };

    void loadProjectRunStates();

    return () => {
      cancelled = true;
    };
  }, [apiClient, enabled, projects]);

  useEffect(() => {
    if (!enabled) {
      setProjectRealtimeLiveById({});
      return;
    }
    setProjectRealtimeLiveById((prev) => {
      const next: Record<string, ProjectRealtimeLiveState> = {};
      (projects || []).forEach((project) => {
        if (prev[project.id]) {
          next[project.id] = prev[project.id];
        }
      });
      return next;
    });
  }, [enabled, projects]);

  const refreshProjectRunMembersState = useCallback(async (projectId: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    if (!enabled || !project) {
      return;
    }
    markProjectRunnerContactRowsStale(apiClient, projectId);
    const nextMemberDetails = await loadProjectRunMembersDetails(apiClient, projectId);
    setProjectRunStateById((prev) => {
      const currentDetails = projectRunStateDetailsRef.current[projectId]
        || createInitialProjectRunStateDetails();
      const nextDetails: ProjectRunStateDetails = {
        ...currentDetails,
        ...nextMemberDetails,
      };
      const next = {
        ...prev,
        [projectId]: buildProjectRunViewState(project, nextDetails),
      };
      projectRunStateRef.current = next;
      projectRunStateDetailsRef.current = {
        ...projectRunStateDetailsRef.current,
        [projectId]: nextDetails,
      };
      return next;
    });
  }, [apiClient, enabled, projects]);

  const refreshProjectRunScriptState = useCallback(async (projectId: string) => {
    const project = (projects || []).find((item) => item.id === projectId);
    if (!enabled || !project) {
      return;
    }
    markProjectRunnerScriptStateStale(apiClient, project.rootPath);
    const nextScriptDetails = await loadProjectRunScriptDetails(apiClient, project);
    setProjectRunStateById((prev) => {
      const currentDetails = projectRunStateDetailsRef.current[projectId]
        || createInitialProjectRunStateDetails();
      const nextDetails: ProjectRunStateDetails = {
        ...currentDetails,
        ...nextScriptDetails,
      };
      const next = {
        ...prev,
        [projectId]: buildProjectRunViewState(project, nextDetails),
      };
      projectRunStateRef.current = next;
      projectRunStateDetailsRef.current = {
        ...projectRunStateDetailsRef.current,
        [projectId]: nextDetails,
      };
      return next;
    });
  }, [apiClient, enabled, projects]);

  useRealtimeEvent((event) => {
    if (!enabled) {
      return;
    }
    if (event.event === 'project.members.updated' && isProjectMembersUpdatedPayload(event)) {
      const payloadProjectId = String(event.project_id || event.payload.project_id || '').trim();
      if (!payloadProjectId || !projectIds.has(payloadProjectId)) {
        return;
      }
      void refreshProjectRunMembersState(payloadProjectId);
      return;
    }
    if (event.event === 'project.run.catalog.updated' && isProjectRunCatalogPayload(event)) {
      const payloadProjectId = String(event.project_id || event.payload.project_id || '').trim();
      if (!payloadProjectId || !projectIds.has(payloadProjectId)) {
        return;
      }
      const project = (projects || []).find((item) => item.id === payloadProjectId);
      if (!project) {
        return;
      }
      const nextScriptDetails = resolveProjectRunScriptSnapshotDetails(apiClient, project, event.payload);
      if (!nextScriptDetails) {
        void refreshProjectRunScriptState(payloadProjectId);
        return;
      }
      setProjectRunStateById((prev) => {
        const currentDetails = projectRunStateDetailsRef.current[payloadProjectId]
          || createInitialProjectRunStateDetails();
        const nextDetails: ProjectRunStateDetails = {
          ...currentDetails,
          ...nextScriptDetails,
        };
        const next = {
          ...prev,
          [payloadProjectId]: buildProjectRunViewState(project, nextDetails),
        };
        projectRunStateRef.current = next;
        projectRunStateDetailsRef.current = {
          ...projectRunStateDetailsRef.current,
          [payloadProjectId]: nextDetails,
        };
        return next;
      });
      return;
    }

    if (event.event !== 'project.run.state_changed' || !isProjectRunStatePayload(event)) {
      return;
    }
    const payloadProjectId = String(event.project_id || event.payload.project_id || '').trim();
    if (!payloadProjectId || !projectIds.has(payloadProjectId)) {
      return;
    }
    const terminalId = readTrimmedString(event.payload.terminal_id);
    const terminalName = readTrimmedString(event.payload.terminal_name) || terminalId || null;
    const cwd = readTrimmedString(event.payload.cwd) || null;
    const status = readTrimmedString(event.payload.status) || 'unknown';
    setProjectRealtimeLiveById((prev) => ({
      ...prev,
      [payloadProjectId]: {
        isRunning: event.payload.running === true,
        terminalId: terminalId || null,
        terminalName,
        cwd,
        busy: event.payload.busy === true,
        status,
      },
    }));
  });

  const projectLiveStateById = useMemo<Record<string, ProjectLiveViewState>>(() => {
    const out: Record<string, ProjectLiveViewState> = {};

    (projects || []).forEach((project) => {
      const { busyTerminal, activeTerminal } = resolveProjectRuntimeTerminal(terminals || [], project.id);
      const realtimeLive = projectRealtimeLiveById[project.id];
      const runState = projectRunStateById[project.id];
      const command = runState?.defaultCommand || runState?.fallbackCommand || null;
      const cwd = runState?.defaultCwd || runState?.fallbackCwd || realtimeLive?.cwd || activeTerminal?.cwd || null;
      const terminalId = realtimeLive?.terminalId || activeTerminal?.id || null;
      const terminalName = realtimeLive?.terminalName || activeTerminal?.name || null;
      const isRunning = typeof realtimeLive?.isRunning === 'boolean'
        ? realtimeLive.isRunning
        : Boolean(busyTerminal);

      out[project.id] = {
        isRunning,
        terminalId,
        terminalName,
        canRestart: Boolean(command && cwd),
        actionLoading: Boolean(projectActionLoadingById[project.id]),
      };
    });

    return out;
  }, [projectActionLoadingById, projectRealtimeLiveById, projectRunStateById, projects, terminals]);

  const setProjectRunError = useCallback((projectId: string, error: string | null) => {
    setProjectRunStateById((prev) => {
      const current = prev[projectId];
      if (!current) {
        return prev;
      }
      const next = {
        ...prev,
        [projectId]: {
          ...current,
          error,
        },
      };
      projectRunStateRef.current = next;
      return next;
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
      if (shouldFallbackRefreshTerminals()) {
        await loadTerminals();
      }
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
      if (shouldFallbackRefreshTerminals()) {
        await loadTerminals();
      }
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
      if (shouldFallbackRefreshTerminals()) {
        await loadTerminals();
      }
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
